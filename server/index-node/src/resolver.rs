use graphql_parser::{query as q, query::Name, schema as s, schema::ObjectType};
use std::collections::{BTreeMap, HashMap};

use graph::data::graphql::{TryFromValue, ValueList, ValueMap};
use graph::data::subgraph::schema::SUBGRAPHS_ID;
use graph::prelude::*;
use graph_graphql::prelude::{object_value, ObjectOrInterface, Resolver};

use web3::types::H256;

/// Resolver for the index node GraphQL API.
pub struct IndexNodeResolver<R, S> {
    logger: Logger,
    graphql_runner: Arc<R>,
    store: Arc<S>,
}

/// The ID of a subgraph deployment assignment.
#[derive(Debug)]
struct DeploymentAssignment {
    /// ID of the subgraph.
    subgraph: String,
    /// ID of the Graph Node that indexes the subgraph.
    node: String,
}

impl TryFromValue for DeploymentAssignment {
    fn try_from_value(value: &q::Value) -> Result<Self, Error> {
        Ok(Self {
            subgraph: value.get_required("id")?,
            node: value.get_required("nodeId")?,
        })
    }
}

/// Light wrapper around `EthereumBlockPointer` that is compatible with GraphQL values.
struct EthereumBlock(EthereumBlockPointer);

impl From<EthereumBlock> for q::Value {
    fn from(block: EthereumBlock) -> Self {
        object_value(vec![
            (
                "__typename",
                q::Value::String(String::from("EthereumBlock")),
            ),
            ("hash", q::Value::String(block.0.hash_hex())),
            ("number", q::Value::String(format!("{}", block.0.number))),
        ])
    }
}

/// The indexing status of a subgraph on an Ethereum network (like mainnet or ropsten).
struct EthereumIndexingStatus {
    /// The network name (e.g. `mainnet`, `ropsten`, `rinkeby`, `kovan` or `goerli`).
    network: String,
    /// The current head block of the chain.
    chain_head_block: Option<EthereumBlock>,
    /// The earliest block available for this subgraph.
    earliest_block: Option<EthereumBlock>,
    /// The latest block that the subgraph has synced to.
    latest_block: Option<EthereumBlock>,
}

/// Indexing status information for different chains (only Ethereum right now).
enum ChainIndexingStatus {
    Ethereum(EthereumIndexingStatus),
}

impl From<ChainIndexingStatus> for q::Value {
    fn from(status: ChainIndexingStatus) -> Self {
        match status {
            ChainIndexingStatus::Ethereum(inner) => object_value(vec![
                // `__typename` is needed for the `ChainIndexingStatus` interface
                // in GraphQL to work.
                (
                    "__typename",
                    q::Value::String(String::from("EthereumIndexingStatus")),
                ),
                ("network", q::Value::String(inner.network)),
                (
                    "chainHeadBlock",
                    inner
                        .chain_head_block
                        .map_or(q::Value::Null, q::Value::from),
                ),
                (
                    "earliestBlock",
                    inner.earliest_block.map_or(q::Value::Null, q::Value::from),
                ),
                (
                    "latestBlock",
                    inner.latest_block.map_or(q::Value::Null, q::Value::from),
                ),
            ]),
        }
    }
}

/// The overall indexing status of a subgraph.
struct IndexingStatusWithoutNode {
    /// The subgraph ID.
    subgraph: String,
    /// Whether or not the subgraph has synced all the way to the current chain head.
    synced: bool,
    /// Whether or not the subgraph has failed syncing.
    failed: bool,
    /// If it has failed, an optional error.
    error: Option<String>,
    /// Indexing status on different chains involved in the subgraph's data sources.
    chains: Vec<ChainIndexingStatus>,
}

struct IndexingStatus {
    /// The subgraph ID.
    subgraph: String,
    /// Whether or not the subgraph has synced all the way to the current chain head.
    synced: bool,
    /// Whether or not the subgraph has failed syncing.
    failed: bool,
    /// If it has failed, an optional error.
    error: Option<String>,
    /// Indexing status on different chains involved in the subgraph's data sources.
    chains: Vec<ChainIndexingStatus>,
    /// ID of the Graph Node that the subgraph is indexed by.
    node: String,
}

impl IndexingStatusWithoutNode {
    /// Adds a Graph Node ID to the indexing status.
    fn with_node(self, node: String) -> IndexingStatus {
        IndexingStatus {
            subgraph: self.subgraph,
            synced: self.synced,
            failed: self.failed,
            error: self.error,
            chains: self.chains,
            node: node,
        }
    }

    /// Attempts to parse `${prefix}Hash` and `${prefix}Number` fields on a
    /// GraphQL object value into an `EthereumBlock`.
    fn block_from_value(
        value: &q::Value,
        prefix: &'static str,
    ) -> Result<Option<EthereumBlock>, Error> {
        let hash_key = format!("{}Hash", prefix);
        let number_key = format!("{}Number", prefix);

        match (
            value.get_optional::<H256>(hash_key.as_ref())?,
            value
                .get_optional::<BigInt>(number_key.as_ref())?
                .map(|n| n.to_u64()),
        ) {
            // Only return an Ethereum block if we can parse both the block hash and number
            (Some(hash), Some(number)) => Ok(Some(EthereumBlock(EthereumBlockPointer {
                hash: hash,
                number: number,
            }))),
            _ => Ok(None),
        }
    }
}

impl TryFromValue for IndexingStatusWithoutNode {
    fn try_from_value(value: &q::Value) -> Result<Self, Error> {
        Ok(Self {
            subgraph: value.get_required("id")?,
            synced: value.get_required("synced")?,
            failed: value.get_required("failed")?,
            error: None,
            chains: vec![ChainIndexingStatus::Ethereum(EthereumIndexingStatus {
                network: value
                    .get_required::<q::Value>("manifest")?
                    .get_required::<q::Value>("dataSources")?
                    .get_values::<q::Value>()?[0]
                    .get_required("network")?,
                chain_head_block: Self::block_from_value(value, "ethereumHeadBlock")?,
                earliest_block: Self::block_from_value(value, "earliestEthereumBlock")?,
                latest_block: Self::block_from_value(value, "latestEthereumBlock")?,
            })],
        })
    }
}

impl From<IndexingStatus> for q::Value {
    fn from(status: IndexingStatus) -> Self {
        object_value(vec![
            (
                "__typename",
                q::Value::String(String::from("SubgraphIndexingStatus")),
            ),
            ("subgraph", q::Value::String(status.subgraph)),
            ("synced", q::Value::Boolean(status.synced)),
            ("failed", q::Value::Boolean(status.failed)),
            (
                "error",
                status.error.map_or(q::Value::Null, q::Value::String),
            ),
            (
                "chains",
                q::Value::List(status.chains.into_iter().map(q::Value::from).collect()),
            ),
            ("node", q::Value::String(status.node)),
        ])
    }
}

struct IndexingStatuses(Vec<IndexingStatus>);

impl From<q::Value> for IndexingStatuses {
    fn from(data: q::Value) -> Self {
        // Extract deployment assignment IDs from the query result
        let assignments = data
            .get_required::<q::Value>("subgraphDeploymentAssignments")
            .expect("no subgraph deployment assignments in the result")
            .get_values::<DeploymentAssignment>()
            .expect("failed to parse subgraph deployment assignments");

        IndexingStatuses(
            // Parse indexing statuses from deployments
            data.get_required::<q::Value>("subgraphDeployments")
                .expect("no subgraph deployments in the result")
                .get_values()
                .expect("failed to parse subgraph deployments")
                .into_iter()
                // Filter out those deployments for which there is no active assignment
                .filter_map(|status: IndexingStatusWithoutNode| {
                    assignments
                        .iter()
                        .find(|assignment| assignment.subgraph == status.subgraph)
                        .map(|assignment| status.with_node(assignment.node.clone()))
                })
                .collect(),
        )
    }
}

impl From<IndexingStatuses> for q::Value {
    fn from(statuses: IndexingStatuses) -> Self {
        q::Value::List(statuses.0.into_iter().map(q::Value::from).collect())
    }
}

impl<R, S> IndexNodeResolver<R, S>
where
    R: GraphQlRunner,
    S: Store + SubgraphDeploymentStore,
{
    pub fn new(logger: &Logger, graphql_runner: Arc<R>, store: Arc<S>) -> Self {
        let logger = logger.new(o!("component" => "IndexNodeResolver"));
        Self {
            logger,
            graphql_runner,
            store,
        }
    }

    fn resolve_indexing_statuses(
        &self,
        arguments: &HashMap<&q::Name, q::Value>,
    ) -> Result<q::Value, QueryExecutionError> {
        // Extract optional "subgraphs" argument
        let subgraphs = arguments
            .get(&String::from("subgraphs"))
            .map(|value| match value {
                ids @ q::Value::List(_) => ids.clone(),
                _ => unreachable!(),
            });

        // Build a `where` filter that both subgraph deployments and subgraph deployment
        // assignments have to match
        let where_filter = object_value(match subgraphs {
            Some(ref ids) => vec![("id_in", ids.clone())],
            None => vec![],
        });

        // Build a query for matching subgraph deployments
        let query = Query {
            // The query is against the subgraph of subgraphs
            schema: self
                .store
                .api_schema(&SUBGRAPHS_ID)
                .map_err(QueryExecutionError::StoreError)?,

            // We're querying all deployments that match the provided filter
            document: q::parse_query(
                r#"
                query deployments(
                  $whereDeployments: SubgraphDeployment_filter!,
                  $whereAssignments: SubgraphDeploymentAssignment_filter!
                ) {
                  subgraphDeployments(where: $whereDeployments, first: 1000000) {
                    id
                    synced
                    failed
                    ethereumHeadBlockNumber
                    ethereumHeadBlockHash
                    earliestEthereumBlockHash
                    earliestEthereumBlockNumber
                    latestEthereumBlockHash
                    latestEthereumBlockNumber
                    manifest {
                      dataSources(first: 1) {
                        network
                      }
                    }
                  }
                  subgraphDeploymentAssignments(where: $whereAssignments, first: 1000000) {
                    id
                    nodeId
                  }
                }
                "#,
            )
            .unwrap(),

            // If the `subgraphs` argument was provided, build a suitable `where`
            // filter to match the IDs; otherwise leave the `where` filter empty
            variables: Some(QueryVariables::new(HashMap::from_iter(
                vec![
                    ("whereDeployments".into(), where_filter.clone()),
                    ("whereAssignments".into(), where_filter),
                ]
                .into_iter(),
            ))),
        };

        // Execute the query
        let result = self
            .graphql_runner
            .run_query_with_complexity(query, None, None, Some(std::u32::MAX))
            .wait()
            .expect("error querying subgraph deployments");

        let data = match result.data {
            Some(data) => data,
            None => {
                error!(
                    self.logger,
                    "Failed to query subgraph deployments";
                    "subgraphs" => format!("{:?}", subgraphs),
                    "errors" => format!("{:?}", result.errors)
                );
                return Ok(q::Value::List(vec![]));
            }
        };

        Ok(IndexingStatuses::from(data).into())
    }

    fn resolve_indexing_statuses_for_subgraph_name(
        &self,
        arguments: &HashMap<&q::Name, q::Value>,
    ) -> Result<q::Value, QueryExecutionError> {
        // Get the subgraph name from the arguments; we can safely use `expect` here
        // because the argument will already have been validated prior to the resolver
        // being called
        let subgraph_name = arguments
            .get_required::<String>("subgraphName")
            .expect("subgraphName not provided");

        debug!(
            self.logger,
            "Resolve indexing statuses for subgraph name";
            "name" => &subgraph_name
        );

        // Build a `where` filter that the subgraph has to match
        let where_filter = object_value(vec![("name", q::Value::String(subgraph_name.clone()))]);

        // Build a query for matching subgraph deployments
        let query = Query {
            // The query is against the subgraph of subgraphs
            schema: self
                .store
                .api_schema(&SUBGRAPHS_ID)
                .map_err(QueryExecutionError::StoreError)?,

            // We're querying all deployments that match the provided filter
            document: q::parse_query(
                r#"
                query subgraphs($where: Subgraph_filter!) {
                  subgraphs(where: $where, first: 1000000) {
                    versions(orderBy: createdAt, orderDirection: asc, first: 1000000) {
                      deployment {
                        id
                        synced
                        failed
                        ethereumHeadBlockNumber
                        ethereumHeadBlockHash
                        earliestEthereumBlockHash
                        earliestEthereumBlockNumber
                        latestEthereumBlockHash
                        latestEthereumBlockNumber
                        manifest {
                          dataSources(first: 1) {
                            network
                          }
                        }
                      }
                    }
                  }
                  subgraphDeploymentAssignments(first: 1000000) {
                    id
                    nodeId
                  }
                }
                "#,
            )
            .unwrap(),

            // If the `subgraphs` argument was provided, build a suitable `where`
            // filter to match the IDs; otherwise leave the `where` filter empty
            variables: Some(QueryVariables::new(HashMap::from_iter(
                vec![("where".into(), where_filter)].into_iter(),
            ))),
        };

        // Execute the query
        let result = self
            .graphql_runner
            .run_query_with_complexity(query, None, None, Some(std::u32::MAX))
            .wait()
            .expect("error querying subgraph deployments");

        let data = match result.data {
            Some(data) => data,
            None => {
                error!(
                    self.logger,
                    "Failed to query subgraph deployments";
                    "subgraph" => subgraph_name,
                    "errors" => format!("{:?}", result.errors)
                );
                return Ok(q::Value::List(vec![]));
            }
        };

        let subgraphs = match data
            .get_optional::<q::Value>("subgraphs")
            .expect("invalid subgraphs")
        {
            Some(subgraphs) => subgraphs,
            None => return Ok(q::Value::List(vec![])),
        };

        let subgraphs = subgraphs
            .get_values::<q::Value>()
            .expect("invalid subgraph values");

        let subgraph = if subgraphs.len() > 0 {
            subgraphs[0].clone()
        } else {
            return Ok(q::Value::List(vec![]));
        };

        let deployments = subgraph
            .get_required::<q::Value>("versions")
            .expect("missing subgraph versions")
            .get_values::<q::Value>()
            .expect("invalid subgraph versions")
            .into_iter()
            .map(|version| {
                version
                    .get_required::<q::Value>("deployment")
                    .expect("missing deployment")
            })
            .collect::<Vec<_>>();

        let transformed_data = object_value(vec![
            ("subgraphDeployments", q::Value::List(deployments)),
            (
                "subgraphDeploymentAssignments",
                data.get_required::<q::Value>("subgraphDeploymentAssignments")
                    .expect("missing deployment assignments"),
            ),
        ]);

        Ok(IndexingStatuses::from(transformed_data).into())
    }
}

impl<R, S> Clone for IndexNodeResolver<R, S>
where
    R: GraphQlRunner,
    S: Store + SubgraphDeploymentStore,
{
    fn clone(&self) -> Self {
        Self {
            logger: self.logger.clone(),
            graphql_runner: self.graphql_runner.clone(),
            store: self.store.clone(),
        }
    }
}

impl<R, S> Resolver for IndexNodeResolver<R, S>
where
    R: GraphQlRunner,
    S: Store + SubgraphDeploymentStore,
{
    fn resolve_objects(
        &self,
        parent: &Option<q::Value>,
        field: &q::Name,
        field_definition: &s::Field,
        object_type: ObjectOrInterface<'_>,
        arguments: &HashMap<&q::Name, q::Value>,
        _types_for_interface: &BTreeMap<Name, Vec<ObjectType>>,
        _max_first: u32,
    ) -> Result<q::Value, QueryExecutionError> {
        match (parent, object_type.name(), field.as_str()) {
            // The top-level `indexingStatuses` field
            (None, "SubgraphIndexingStatus", "indexingStatuses") => {
                self.resolve_indexing_statuses(arguments)
            }

            // The `chains` field of `ChainIndexingStatus` values
            (Some(status), "ChainIndexingStatus", "chains") => match status {
                q::Value::Object(map) => Ok(map
                    .get("chains")
                    .expect("subgraph indexing status without `chains`")
                    .clone()),
                _ => unreachable!(),
            },

            // The top-level `indexingStatusesForSubgraphName` field
            (None, "SubgraphIndexingStatus", "indexingStatusesForSubgraphName") => {
                self.resolve_indexing_statuses_for_subgraph_name(arguments)
            }

            // Unknown fields on the `Query` type
            (None, _, name) => Err(QueryExecutionError::UnknownField(
                field_definition.position.clone(),
                "Query".into(),
                name.into(),
            )),

            // Unknown fields on any other types
            (_, type_name, name) => Err(QueryExecutionError::UnknownField(
                field_definition.position.clone(),
                type_name.into(),
                name.into(),
            )),
        }
    }

    fn resolve_object(
        &self,
        parent: &Option<q::Value>,
        field: &q::Field,
        field_definition: &s::Field,
        object_type: ObjectOrInterface<'_>,
        _arguments: &HashMap<&q::Name, q::Value>,
        _types_for_interface: &BTreeMap<Name, Vec<ObjectType>>,
    ) -> Result<q::Value, QueryExecutionError> {
        match (parent, object_type.name(), field.name.as_str()) {
            (Some(status), "EthereumBlock", "chainHeadBlock") => Ok(status
                .get_optional("chainHeadBlock")
                .map_err(|e| QueryExecutionError::StoreError(e))?
                .unwrap_or(q::Value::Null)),
            (Some(status), "EthereumBlock", "earliestBlock") => Ok(status
                .get_optional("earliestBlock")
                .map_err(|e| QueryExecutionError::StoreError(e))?
                .unwrap_or(q::Value::Null)),
            (Some(status), "EthereumBlock", "latestBlock") => Ok(status
                .get_optional("latestBlock")
                .map_err(|e| QueryExecutionError::StoreError(e))?
                .unwrap_or(q::Value::Null)),

            // Unknown fields on other types
            (_, type_name, name) => Err(QueryExecutionError::UnknownField(
                field_definition.position.clone(),
                type_name.into(),
                name.into(),
            )),
        }
    }
}
