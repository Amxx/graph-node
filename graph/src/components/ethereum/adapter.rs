use ethabi::{Bytes, Error as ABIError, Function, ParamType, Token};
use failure::SyncFailure;
use futures::Future;
use petgraph::graphmap::GraphMap;
use std::cmp;
use std::collections::{HashMap, HashSet};
use std::fmt;
use tiny_keccak::keccak256;
use web3::types::*;

use super::types::*;
use crate::components::metrics::{CounterVec, GaugeVec, HistogramVec};
use crate::prelude::*;

pub type EventSignature = H256;

/// A collection of attributes that (kind of) uniquely identify an Ethereum blockchain.
pub struct EthereumNetworkIdentifier {
    pub net_version: String,
    pub genesis_block_hash: H256,
}

/// A request for the state of a contract at a specific block hash and address.
pub struct EthereumContractStateRequest {
    pub address: Address,
    pub block_hash: H256,
}

/// An error that can occur when trying to obtain the state of a contract.
pub enum EthereumContractStateError {
    Failed,
}

/// Representation of an Ethereum contract state.
pub struct EthereumContractState {
    pub address: Address,
    pub block_hash: H256,
    pub data: Bytes,
}

#[derive(Clone, Debug)]
pub struct EthereumContractCall {
    pub address: Address,
    pub block_ptr: EthereumBlockPointer,
    pub function: Function,
    pub args: Vec<Token>,
}

#[derive(Fail, Debug)]
pub enum EthereumContractCallError {
    #[fail(display = "ABI error: {}", _0)]
    ABIError(SyncFailure<ABIError>),
    /// `Token` is not of expected `ParamType`
    #[fail(display = "type mismatch, token {:?} is not of kind {:?}", _0, _1)]
    TypeError(Token, ParamType),
    #[fail(display = "call error: {}", _0)]
    Web3Error(web3::Error),
    #[fail(display = "call reverted: {}", _0)]
    Revert(String),
    #[fail(display = "ethereum node took too long to perform call")]
    Timeout,
}

impl From<ABIError> for EthereumContractCallError {
    fn from(e: ABIError) -> Self {
        EthereumContractCallError::ABIError(SyncFailure::new(e))
    }
}

#[derive(Fail, Debug)]
pub enum EthereumAdapterError {
    /// The Ethereum node does not know about this block for some reason, probably because it
    /// disappeared in a chain reorg.
    #[fail(
        display = "Block data unavailable, block was likely uncled (block hash = {:?})",
        _0
    )]
    BlockUnavailable(H256),

    /// An unexpected error occurred.
    #[fail(display = "Ethereum adapter error: {}", _0)]
    Unknown(Error),
}

impl From<Error> for EthereumAdapterError {
    fn from(e: Error) -> Self {
        EthereumAdapterError::Unknown(e)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum LogFilterNode {
    Contract(Address),
    Event(EventSignature),
}

/// Corresponds to an `eth_getLogs` call.
#[derive(Clone)]
pub struct EthGetLogsFilter {
    pub contracts: Vec<Address>,
    pub event_signatures: Vec<EventSignature>,
}

impl fmt::Display for EthGetLogsFilter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.contracts.len() == 1 {
            write!(
                f,
                "contract {:?}, {} events",
                self.contracts[0],
                self.event_signatures.len()
            )
        } else if self.event_signatures.len() == 1 {
            write!(
                f,
                "event {:?}, {} contracts",
                self.event_signatures[0],
                self.contracts.len()
            )
        } else {
            write!(f, "unreachable")
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumLogFilter {
    /// Log filters can be represented as a bipartite graph between contracts and events. An edge
    /// exists between a contract and an event if a data source for the contract has a trigger for
    /// the event.
    contracts_and_events_graph: GraphMap<LogFilterNode, (), petgraph::Undirected>,

    // Event sigs with no associated address, matching on all addresses.
    wildcard_events: HashSet<EventSignature>,
}

impl EthereumLogFilter {
    /// Check if log bloom filter indicates a possible match for this log filter.
    /// Returns `true` to indicate that a matching `Log` _might_ be contained.
    /// Returns `false` to indicate that a matching `Log` _is not_ contained.
    pub fn check_bloom(&self, _bloom: H2048) -> bool {
        // TODO issue #352: implement bloom filter check
        true // not even wrong
    }

    /// Check if this filter matches the specified `Log`.
    pub fn matches(&self, log: &Log) -> bool {
        // First topic should be event sig
        match log.topics.first() {
            None => false,

            Some(sig) => {
                // The `Log` matches the filter either if the filter contains
                // a (contract address, event signature) pair that matches the
                // `Log`, or if the filter contains wildcard event that matches.
                let contract = LogFilterNode::Contract(log.address.clone());
                let event = LogFilterNode::Event(*sig);
                self.contracts_and_events_graph
                    .all_edges()
                    .any(|(s, t, ())| {
                        (s == contract && t == event) || (t == contract && s == event)
                    })
                    || self.wildcard_events.contains(sig)
            }
        }
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        let mut this = EthereumLogFilter::default();
        for ds in iter {
            for event_sig in ds.mapping.event_handlers.iter().map(|e| e.topic0()) {
                match ds.source.address {
                    Some(contract) => {
                        this.contracts_and_events_graph.add_edge(
                            LogFilterNode::Contract(contract),
                            LogFilterNode::Event(event_sig),
                            (),
                        );
                    }
                    None => {
                        this.wildcard_events.insert(event_sig);
                    }
                }
            }
        }
        this
    }

    /// Extends this log filter with another one.
    pub fn extend(&mut self, other: EthereumLogFilter) {
        // Destructure to make sure we're checking all fields.
        let EthereumLogFilter {
            contracts_and_events_graph,
            wildcard_events,
        } = other;
        for (s, t, ()) in contracts_and_events_graph.all_edges() {
            self.contracts_and_events_graph.add_edge(s, t, ());
        }
        self.wildcard_events.extend(wildcard_events);
    }

    /// An empty filter is one that never matches.
    pub fn is_empty(&self) -> bool {
        // Destructure to make sure we're checking all fields.
        let EthereumLogFilter {
            contracts_and_events_graph,
            wildcard_events,
        } = self;
        contracts_and_events_graph.edge_count() == 0 && wildcard_events.is_empty()
    }

    /// Filters for `eth_getLogs` calls. The filters will not return false positives. This attempts
    /// to balance between having granular filters but too many calls and having few calls but too
    /// broad filters causing the Ethereum endpoint to timeout.
    pub fn eth_get_logs_filters(self) -> impl Iterator<Item = EthGetLogsFilter> {
        let mut filters = Vec::new();

        // First add the wildcard event filters.
        for wildcard_event in self.wildcard_events {
            filters.push(EthGetLogsFilter {
                contracts: vec![],
                event_signatures: vec![wildcard_event],
            })
        }

        // The current algorithm is to repeatedly find the maximum cardinality vertex and turn all
        // of its edges into a filter. This is nice because it is neutral between filtering by
        // contract or by events, if there are many events that appear on only one data source
        // we'll filter by many events on a single contract, but if there is an event that appears
        // on a lot of data sources we'll filter by many contracts with a single event.
        //
        // From a theoretical standpoint we're finding a vertex cover, and this is not the optimal
        // algorithm to find a minimum vertex cover, but should be fine as an approximation.
        //
        // One optimization we're not doing is to merge nodes that have the same neighbors into a
        // single node. For example if a subgraph has two data sources, each with the same two
        // events, we could cover that with a single filter and no false positives. However that
        // might cause the filter to become too broad, so at the moment it seems excessive.
        let mut g = self.contracts_and_events_graph;
        while g.edge_count() > 0 {
            // If there are edges, there are vertexes.
            let max_vertex = g.nodes().max_by_key(|&n| g.neighbors(n).count()).unwrap();
            let mut filter = match max_vertex {
                LogFilterNode::Contract(address) => EthGetLogsFilter {
                    contracts: vec![address],
                    event_signatures: vec![],
                },
                LogFilterNode::Event(event_sig) => EthGetLogsFilter {
                    contracts: vec![],
                    event_signatures: vec![event_sig],
                },
            };
            for neighbor in g.neighbors(max_vertex) {
                match neighbor {
                    LogFilterNode::Contract(address) => filter.contracts.push(address),
                    LogFilterNode::Event(event_sig) => filter.event_signatures.push(event_sig),
                }
            }

            // Sanity checks:
            // - The filter is not a wildcard because all nodes have neighbors.
            // - The graph is bipartite.
            assert!(filter.contracts.len() > 0 && filter.event_signatures.len() > 0);
            assert!(filter.contracts.len() == 1 || filter.event_signatures.len() == 1);
            filters.push(filter);
            g.remove_node(max_vertex);
        }
        filters.into_iter()
    }
}

#[derive(Clone, Debug)]
pub struct EthereumCallFilter {
    // Each call filter has a map of filters keyed by address, each containing a tuple with
    // start_block and the set of function signatures
    pub contract_addresses_function_signatures: HashMap<Address, (u64, HashSet<[u8; 4]>)>,
}

impl EthereumCallFilter {
    pub fn matches(&self, call: &EthereumCall) -> bool {
        // Ensure the call is to a contract the filter expressed an interest in
        if !self
            .contract_addresses_function_signatures
            .contains_key(&call.to)
        {
            return false;
        }
        // If the call is to a contract with no specified functions, keep the call
        if self
            .contract_addresses_function_signatures
            .get(&call.to)
            .unwrap()
            .1
            .is_empty()
        {
            // Allow the ability to match on calls to a contract generally
            // If you want to match on a generic call to contract this limits you
            // from matching with a specific call to a contract
            return true;
        }
        // Ensure the call is to run a function the filter expressed an interest in
        self.contract_addresses_function_signatures
            .get(&call.to)
            .unwrap()
            .1
            .contains(&call.input.0[..4])
    }

    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        iter.into_iter()
            .filter_map(|data_source| data_source.source.address.map(|addr| (addr, data_source)))
            .map(|(contract_addr, data_source)| {
                let start_block = data_source.source.start_block;
                data_source
                    .mapping
                    .call_handlers
                    .iter()
                    .map(move |call_handler| {
                        let sig = keccak256(call_handler.function.as_bytes());
                        (start_block, contract_addr, [sig[0], sig[1], sig[2], sig[3]])
                    })
            })
            .flatten()
            .collect()
    }

    /// Extends this call filter with another one.
    pub fn extend(&mut self, other: EthereumCallFilter) {
        // Extend existing address / function signature key pairs
        // Add new address / function signature key pairs from the provided EthereumCallFilter
        for (address, (proposed_start_block, new_sigs)) in
            other.contract_addresses_function_signatures.into_iter()
        {
            match self
                .contract_addresses_function_signatures
                .get_mut(&address)
            {
                Some((existing_start_block, existing_sigs)) => {
                    *existing_start_block =
                        cmp::min(proposed_start_block, existing_start_block.clone());
                    existing_sigs.extend(new_sigs);
                }
                None => {
                    self.contract_addresses_function_signatures
                        .insert(address, (proposed_start_block, new_sigs));
                }
            }
        }
    }

    /// An empty filter is one that never matches.
    pub fn is_empty(&self) -> bool {
        // Destructure to make sure we're checking all fields.
        let EthereumCallFilter {
            contract_addresses_function_signatures,
        } = self;
        contract_addresses_function_signatures.is_empty()
    }

    pub fn start_blocks(&self) -> Vec<u64> {
        self.contract_addresses_function_signatures
            .values()
            .filter(|(start_block, _fn_sigs)| start_block > &0)
            .map(|(start_block, _fn_sigs)| *start_block)
            .collect()
    }
}

impl FromIterator<(u64, Address, [u8; 4])> for EthereumCallFilter {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (u64, Address, [u8; 4])>,
    {
        let mut lookup: HashMap<Address, (u64, HashSet<[u8; 4]>)> = HashMap::new();
        iter.into_iter()
            .for_each(|(start_block, address, function_signature)| {
                if !lookup.contains_key(&address) {
                    lookup.insert(address, (start_block, HashSet::default()));
                }
                lookup.get_mut(&address).map(|set| {
                    if set.0 > start_block {
                        set.0 = start_block
                    }
                    set.1.insert(function_signature);
                    set
                });
            });
        EthereumCallFilter {
            contract_addresses_function_signatures: lookup,
        }
    }
}

impl From<EthereumBlockFilter> for EthereumCallFilter {
    fn from(ethereum_block_filter: EthereumBlockFilter) -> Self {
        Self {
            contract_addresses_function_signatures: ethereum_block_filter
                .contract_addresses
                .into_iter()
                .map(|(start_block_opt, address)| (address, (start_block_opt, HashSet::default())))
                .collect::<HashMap<Address, (u64, HashSet<[u8; 4]>)>>(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumBlockFilter {
    pub contract_addresses: HashSet<(u64, Address)>,
    pub trigger_every_block: bool,
}

impl EthereumBlockFilter {
    pub fn from_data_sources<'a>(iter: impl IntoIterator<Item = &'a DataSource>) -> Self {
        iter.into_iter()
            .filter(|data_source| data_source.source.address.is_some())
            .fold(Self::default(), |mut filter_opt, data_source| {
                let has_block_handler_with_call_filter = data_source
                    .mapping
                    .block_handlers
                    .clone()
                    .into_iter()
                    .any(|block_handler| match block_handler.filter {
                        Some(ref filter) if *filter == BlockHandlerFilter::Call => return true,
                        _ => return false,
                    });

                let has_block_handler_without_filter = data_source
                    .mapping
                    .block_handlers
                    .clone()
                    .into_iter()
                    .any(|block_handler| block_handler.filter.is_none());

                filter_opt.extend(Self {
                    trigger_every_block: has_block_handler_without_filter,
                    contract_addresses: if has_block_handler_with_call_filter {
                        vec![(
                            data_source.source.start_block,
                            data_source.source.address.unwrap().to_owned(),
                        )]
                        .into_iter()
                        .collect()
                    } else {
                        HashSet::default()
                    },
                });
                filter_opt
            })
    }

    pub fn extend(&mut self, other: EthereumBlockFilter) {
        self.trigger_every_block = self.trigger_every_block || other.trigger_every_block;
        self.contract_addresses = self.contract_addresses.iter().cloned().fold(
            HashSet::new(),
            |mut addresses, (start_block, address)| {
                match other
                    .contract_addresses
                    .iter()
                    .cloned()
                    .find(|(_, other_address)| &address == other_address)
                {
                    Some((other_start_block, address)) => {
                        addresses.insert((cmp::min(other_start_block, start_block), address));
                    }
                    None => {
                        addresses.insert((start_block, address));
                    }
                }
                addresses
            },
        );
    }

    pub fn start_blocks(&self) -> Vec<u64> {
        self.contract_addresses
            .iter()
            .cloned()
            .filter(|(start_block, _fn_sigs)| start_block > &0)
            .map(|(start_block, _fn_sigs)| start_block)
            .collect()
    }
}

#[derive(Clone)]
pub struct ProviderEthRpcMetrics {
    request_duration: Box<HistogramVec>,
    errors: Box<CounterVec>,
}

impl ProviderEthRpcMetrics {
    pub fn new<M: MetricsRegistry>(registry: Arc<M>) -> Self {
        let request_duration = registry
            .new_histogram_vec(
                String::from("eth_rpc_request_duration"),
                String::from("Measures eth rpc request duration"),
                HashMap::new(),
                vec![String::from("method")],
                vec![0.05, 0.2, 0.5, 1.0, 3.0, 5.0],
            )
            .unwrap();
        let errors = registry
            .new_counter_vec(
                String::from("eth_rpc_errors"),
                String::from("Counts eth rpc request errors"),
                HashMap::new(),
                vec![String::from("method")],
            )
            .unwrap();
        Self {
            request_duration,
            errors,
        }
    }

    pub fn observe_request(&self, duration: f64, method: &str) {
        self.request_duration
            .with_label_values(vec![method].as_slice())
            .observe(duration);
    }

    pub fn add_error(&self, method: &str) {
        self.errors.with_label_values(vec![method].as_slice()).inc();
    }
}

#[derive(Clone)]
pub struct SubgraphEthRpcMetrics {
    request_duration: Box<GaugeVec>,
    errors: Box<CounterVec>,
}

impl SubgraphEthRpcMetrics {
    pub fn new<M: MetricsRegistry>(registry: Arc<M>, subgraph_hash: String) -> Self {
        let request_duration = registry
            .new_gauge_vec(
                format!("subgraph_eth_rpc_request_duration_{}", subgraph_hash),
                String::from("Measures eth rpc request duration for a subgraph deployment"),
                HashMap::new(),
                vec![String::from("method")],
            )
            .unwrap();
        let errors = registry
            .new_counter_vec(
                format!("subgraph_eth_rpc_errors_{}", subgraph_hash),
                String::from("Counts eth rpc request errors for a subgraph deployment"),
                HashMap::new(),
                vec![String::from("method")],
            )
            .unwrap();
        Self {
            request_duration,
            errors,
        }
    }

    pub fn observe_request(&self, duration: f64, method: &str) {
        self.request_duration
            .with_label_values(vec![method].as_slice())
            .set(duration);
    }

    pub fn add_error(&self, method: &str) {
        self.errors.with_label_values(vec![method].as_slice()).inc();
    }
}

#[derive(Clone)]
pub struct BlockStreamMetrics {
    pub ethrpc_metrics: Arc<SubgraphEthRpcMetrics>,
    pub blocks_behind: Box<Gauge>,
    pub reverted_blocks: Box<Gauge>,
    pub stopwatch: StopwatchMetrics,
}

impl BlockStreamMetrics {
    pub fn new<M: MetricsRegistry>(
        registry: Arc<M>,
        ethrpc_metrics: Arc<SubgraphEthRpcMetrics>,
        deployment_id: SubgraphDeploymentId,
        stopwatch: StopwatchMetrics,
    ) -> Self {
        let blocks_behind = registry
            .new_gauge(
                format!("subgraph_blocks_behind_{}", deployment_id.to_string()),
                String::from(
                    "Track the number of blocks a subgraph deployment is behind the HEAD block",
                ),
                HashMap::new(),
            )
            .expect("failed to create `subgraph_blocks_behind` gauge");
        let reverted_blocks = registry
            .new_gauge(
                format!("subgraph_reverted_blocks_{}", deployment_id.to_string()),
                String::from("Track the last reverted block for a subgraph deployment"),
                HashMap::new(),
            )
            .expect("Failed to create `subgraph_reverted_blocks` gauge");
        Self {
            ethrpc_metrics,
            blocks_behind,
            reverted_blocks,
            stopwatch,
        }
    }
}

/// Common trait for components that watch and manage access to Ethereum.
///
/// Implementations may be implemented against an in-process Ethereum node
/// or a remote node over RPC.
pub trait EthereumAdapter: Send + Sync + 'static {
    /// Ask the Ethereum node for some identifying information about the Ethereum network it is
    /// connected to.
    fn net_identifiers(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = EthereumNetworkIdentifier, Error = Error> + Send>;

    /// Find the most recent block.
    fn latest_block(
        &self,
        logger: &Logger,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = EthereumAdapterError> + Send>;

    fn load_block(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = LightEthereumBlock, Error = Error> + Send>;

    /// Load Ethereum blocks in bulk, returning results as they come back as a Stream.
    /// May use the `chain_store` as a cache.
    fn load_blocks(
        &self,
        logger: Logger,
        chain_store: Arc<dyn ChainStore>,
        block_hashes: HashSet<H256>,
    ) -> Box<dyn Stream<Item = LightEthereumBlock, Error = Error> + Send>;

    /// Reorg safety: `to` must be a final block.
    fn block_range_to_ptrs(
        &self,
        logger: Logger,
        from: u64,
        to: u64,
    ) -> Box<dyn Future<Item = Vec<EthereumBlockPointer>, Error = Error> + Send>;

    /// Find a block by its hash.
    fn block_by_hash(
        &self,
        logger: &Logger,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Option<LightEthereumBlock>, Error = Error> + Send>;

    /// Load full information for the specified `block` (in particular, transaction receipts).
    fn load_full_block(
        &self,
        logger: &Logger,
        block: LightEthereumBlock,
    ) -> Box<dyn Future<Item = EthereumBlock, Error = EthereumAdapterError> + Send>;

    /// Load block pointer for the specified `block number`.
    fn block_pointer_from_number(
        &self,
        logger: &Logger,
        block_number: u64,
    ) -> Box<dyn Future<Item = EthereumBlockPointer, Error = EthereumAdapterError> + Send>;

    /// Find a block by its number.
    ///
    /// Careful: don't use this function without considering race conditions.
    /// Chain reorgs could happen at any time, and could affect the answer received.
    /// Generally, it is only safe to use this function with blocks that have received enough
    /// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
    /// those confirmations.
    /// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
    /// reorgs.
    fn block_hash_by_block_number(
        &self,
        logger: &Logger,
        block_number: u64,
    ) -> Box<dyn Future<Item = Option<H256>, Error = Error> + Send>;

    /// Check if `block_ptr` refers to a block that is on the main chain, according to the Ethereum
    /// node.
    ///
    /// Careful: don't use this function without considering race conditions.
    /// Chain reorgs could happen at any time, and could affect the answer received.
    /// Generally, it is only safe to use this function with blocks that have received enough
    /// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
    /// those confirmations.
    /// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
    /// reorgs.
    fn is_on_main_chain(
        &self,
        logger: &Logger,
        metrics: Arc<SubgraphEthRpcMetrics>,
        block_ptr: EthereumBlockPointer,
    ) -> Box<dyn Future<Item = bool, Error = Error> + Send>;

    fn calls_in_block(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        block_number: u64,
        block_hash: H256,
    ) -> Box<dyn Future<Item = Vec<EthereumCall>, Error = Error> + Send>;

    /// Returns blocks with triggers, corresponding to the specified range and filters.
    /// If a block contains no triggers, there may be no corresponding item in the stream.
    /// However the `to` block will always be present, even if triggers are empty.
    ///
    /// Careful: don't use this function without considering race conditions.
    /// Chain reorgs could happen at any time, and could affect the answer received.
    /// Generally, it is only safe to use this function with blocks that have received enough
    /// confirmations to guarantee no further reorgs, **and** where the Ethereum node is aware of
    /// those confirmations.
    /// If the Ethereum node is far behind in processing blocks, even old blocks can be subject to
    /// reorgs.
    /// It is recommended that `to` be far behind the block number of latest block the Ethereum
    /// node is aware of.
    fn blocks_with_triggers(
        self: Arc<Self>,
        logger: Logger,
        chain_store: Arc<dyn ChainStore>,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: u64,
        to: u64,
        log_filter: EthereumLogFilter,
        call_filter: EthereumCallFilter,
        block_filter: EthereumBlockFilter,
    ) -> Box<dyn Future<Item = Vec<EthereumBlockWithTriggers>, Error = Error> + Send> {
        // Each trigger filter needs to be queried for the same block range
        // and the blocks yielded need to be deduped. If any error occurs
        // while searching for a trigger type, the entire operation fails.
        let eth = self.clone();
        let mut trigger_futs: futures::stream::FuturesUnordered<
            Box<dyn Future<Item = Vec<EthereumTrigger>, Error = Error> + Send>,
        > = futures::stream::FuturesUnordered::new();

        // Scan the block range from triggers to find relevant blocks
        if !log_filter.is_empty() {
            trigger_futs.push(Box::new(
                eth.logs_in_block_range(&logger, subgraph_metrics.clone(), from, to, log_filter)
                    .map(|logs: Vec<Log>| logs.into_iter().map(EthereumTrigger::Log).collect()),
            ))
        }

        if !call_filter.is_empty() {
            trigger_futs.push(Box::new(
                eth.calls_in_block_range(&logger, subgraph_metrics.clone(), from, to, call_filter)
                    .map(EthereumTrigger::Call)
                    .collect(),
            ));
        }

        if block_filter.trigger_every_block {
            trigger_futs.push(Box::new(
                self.block_range_to_ptrs(logger.clone(), from, to)
                    .map(move |ptrs| {
                        ptrs.into_iter()
                            .map(|ptr| EthereumTrigger::Block(ptr, EthereumBlockTriggerType::Every))
                            .collect()
                    }),
            ))
        } else if !block_filter.contract_addresses.is_empty() {
            // To determine which blocks include a call to addresses
            // in the block filter, transform the `block_filter` into
            // a `call_filter` and run `blocks_with_calls`
            let call_filter = EthereumCallFilter::from(block_filter);
            trigger_futs.push(Box::new(
                eth.calls_in_block_range(&logger, subgraph_metrics.clone(), from, to, call_filter)
                    .map(|call| {
                        EthereumTrigger::Block(
                            EthereumBlockPointer::from(&call),
                            EthereumBlockTriggerType::WithCallTo(call.to),
                        )
                    })
                    .collect(),
            ));
        }

        let logger1 = logger.clone();
        Box::new(
            trigger_futs
                .concat2()
                .join(self.clone().block_hash_by_block_number(&logger, to))
                .map(move |(triggers, to_hash)| {
                    let mut block_hashes: HashSet<H256> =
                        triggers.iter().map(EthereumTrigger::block_hash).collect();
                    let mut triggers_by_block: HashMap<u64, Vec<EthereumTrigger>> =
                        triggers.into_iter().fold(HashMap::new(), |mut map, t| {
                            map.entry(t.block_number()).or_default().push(t);
                            map
                        });

                    debug!(logger, "Found {} relevant block(s)", block_hashes.len());

                    // Make sure `to` is included, even if empty.
                    block_hashes.insert(to_hash.unwrap());
                    triggers_by_block.entry(to).or_insert(Vec::new());

                    (block_hashes, triggers_by_block)
                })
                .and_then(move |(block_hashes, mut triggers_by_block)| {
                    self.load_blocks(logger1, chain_store, block_hashes)
                        .map(move |block| {
                            EthereumBlockWithTriggers::new(
                                // All blocks with triggers are in `triggers_by_block`, and will be
                                // accessed here exactly once.
                                triggers_by_block.remove(&block.number()).unwrap(),
                                BlockFinality::Final(block),
                            )
                        })
                        .collect()
                        .map(|mut blocks| {
                            blocks.sort_by_key(|block| block.ethereum_block.number());
                            blocks
                        })
                }),
        )
    }

    fn logs_in_block_range(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: u64,
        to: u64,
        log_filter: EthereumLogFilter,
    ) -> Box<dyn Future<Item = Vec<Log>, Error = Error> + Send>;

    fn calls_in_block_range(
        &self,
        logger: &Logger,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        from: u64,
        to: u64,
        call_filter: EthereumCallFilter,
    ) -> Box<dyn Stream<Item = EthereumCall, Error = Error> + Send>;

    /// Call the function of a smart contract.
    fn contract_call(
        &self,
        logger: &Logger,
        call: EthereumContractCall,
        cache: Arc<dyn EthereumCallCache>,
    ) -> Box<dyn Future<Item = Vec<Token>, Error = EthereumContractCallError> + Send>;

    fn triggers_in_block(
        self: Arc<Self>,
        logger: Logger,
        chain_store: Arc<dyn ChainStore>,
        subgraph_metrics: Arc<SubgraphEthRpcMetrics>,
        log_filter: EthereumLogFilter,
        call_filter: EthereumCallFilter,
        block_filter: EthereumBlockFilter,
        ethereum_block: BlockFinality,
    ) -> Box<dyn Future<Item = EthereumBlockWithTriggers, Error = Error> + Send>;
}

#[cfg(test)]
mod tests {
    use super::EthereumCallFilter;

    use web3::types::Address;

    use std::collections::{HashMap, HashSet};
    use std::iter::FromIterator;

    #[test]
    fn extending_ethereum_call_filter() {
        let mut base = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![
                (
                    Address::from_low_u64_be(0),
                    (0, HashSet::from_iter(vec![[0u8; 4]])),
                ),
                (
                    Address::from_low_u64_be(1),
                    (1, HashSet::from_iter(vec![[1u8; 4]])),
                ),
            ]),
        };
        let extension = EthereumCallFilter {
            contract_addresses_function_signatures: HashMap::from_iter(vec![
                (
                    Address::from_low_u64_be(0),
                    (2, HashSet::from_iter(vec![[2u8; 4]])),
                ),
                (
                    Address::from_low_u64_be(3),
                    (3, HashSet::from_iter(vec![[3u8; 4]])),
                ),
            ]),
        };
        base.extend(extension);

        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(0)),
            Some(&(0, HashSet::from_iter(vec![[0u8; 4], [2u8; 4]])))
        );
        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(3)),
            Some(&(3, HashSet::from_iter(vec![[3u8; 4]])))
        );
        assert_eq!(
            base.contract_addresses_function_signatures
                .get(&Address::from_low_u64_be(1)),
            Some(&(1, HashSet::from_iter(vec![[1u8; 4]])))
        );
    }
}
