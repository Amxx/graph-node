#[macro_use]
extern crate pretty_assertions;

use graphql_parser::{query as q, schema as s};
use std::collections::{BTreeMap, HashMap};

use graph::prelude::*;
use graph_graphql::prelude::*;

/// Mock resolver used in tests that don't need a resolver.
#[derive(Clone)]
pub struct MockResolver;

impl Resolver for MockResolver {
    fn resolve_objects<'a>(
        &self,
        _parent: &Option<q::Value>,
        _field: &q::Name,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
        _arguments: &HashMap<&q::Name, q::Value>,
        _types_for_interface: &BTreeMap<Name, Vec<ObjectType>>,
        _max_first: u32,
    ) -> Result<q::Value, QueryExecutionError> {
        Ok(q::Value::Null)
    }

    fn resolve_object(
        &self,
        _parent: &Option<q::Value>,
        _field: &q::Field,
        _field_definition: &s::Field,
        _object_type: ObjectOrInterface<'_>,
        _arguments: &HashMap<&q::Name, q::Value>,
        _types_for_interface: &BTreeMap<Name, Vec<ObjectType>>,
    ) -> Result<q::Value, QueryExecutionError> {
        Ok(q::Value::Null)
    }
}

/// Creates a basic GraphQL schema that exercies scalars, directives,
/// enums, interfaces, input objects, object types and field arguments.
fn mock_schema() -> Schema {
    Schema::parse(
        "
             scalar ID
             scalar Int
             scalar String
             scalar Boolean

             directive @language(
               language: String = \"English\"
             ) on FIELD_DEFINITION

             enum Role {
               USER
               ADMIN
             }

             interface Node {
               id: ID!
             }

             type User implements Node @entity {
               id: ID!
               name: String! @language(language: \"English\")
               role: Role!
             }

             enum User_orderBy {
               id
               name
             }

             input User_filter {
               name_eq: String = \"default name\",
               name_not: String,
             }

             type Query @entity {
               allUsers(orderBy: User_orderBy, filter: User_filter): [User!]
               anyUserWithAge(age: Int = 99): User
               User: User
             }
             ",
        SubgraphDeploymentId::new("mockschema").unwrap(),
    )
    .unwrap()
}

/// Builds the expected result for GraphiQL's introspection query that we are
/// using for testing.
fn expected_mock_schema_introspection() -> q::Value {
    let string_type = object_value(vec![
        ("kind", q::Value::Enum("SCALAR".to_string())),
        ("name", q::Value::String("String".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let id_type = object_value(vec![
        ("kind", q::Value::Enum("SCALAR".to_string())),
        ("name", q::Value::String("ID".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let int_type = object_value(vec![
        ("kind", q::Value::Enum("SCALAR".to_string())),
        ("name", q::Value::String("Int".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let boolean_type = object_value(vec![
        ("kind", q::Value::Enum("SCALAR".to_string())),
        ("name", q::Value::String("Boolean".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let role_type = object_value(vec![
        ("kind", q::Value::Enum("ENUM".to_string())),
        ("name", q::Value::String("Role".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        (
            "enumValues",
            q::Value::List(vec![
                object_value(vec![
                    ("name", q::Value::String("USER".to_string())),
                    ("description", q::Value::Null),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("ADMIN".to_string())),
                    ("description", q::Value::Null),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
            ]),
        ),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let node_type = object_value(vec![
        ("kind", q::Value::Enum("INTERFACE".to_string())),
        ("name", q::Value::String("Node".to_string())),
        ("description", q::Value::Null),
        (
            "fields",
            q::Value::List(vec![object_value(vec![
                ("name", q::Value::String("id".to_string())),
                ("description", q::Value::Null),
                (
                    "type",
                    object_value(vec![
                        ("kind", q::Value::Enum("NON_NULL".to_string())),
                        ("name", q::Value::Null),
                        (
                            "ofType",
                            object_value(vec![
                                ("kind", q::Value::Enum("SCALAR".to_string())),
                                ("name", q::Value::String("ID".to_string())),
                                ("ofType", q::Value::Null),
                            ]),
                        ),
                    ]),
                ),
                ("args", q::Value::List(vec![])),
                ("deprecationReason", q::Value::Null),
                ("isDeprecated", q::Value::Boolean(false)),
            ])]),
        ),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        (
            "possibleTypes",
            q::Value::List(vec![object_value(vec![
                ("kind", q::Value::Enum("OBJECT".to_string())),
                ("name", q::Value::String("User".to_string())),
                ("ofType", q::Value::Null),
            ])]),
        ),
    ]);

    let user_orderby_type = object_value(vec![
        ("kind", q::Value::Enum("ENUM".to_string())),
        ("name", q::Value::String("User_orderBy".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        ("inputFields", q::Value::Null),
        (
            "enumValues",
            q::Value::List(vec![
                object_value(vec![
                    ("name", q::Value::String("id".to_string())),
                    ("description", q::Value::Null),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("name".to_string())),
                    ("description", q::Value::Null),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
            ]),
        ),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let user_filter_type = object_value(vec![
        ("kind", q::Value::Enum("INPUT_OBJECT".to_string())),
        ("name", q::Value::String("User_filter".to_string())),
        ("description", q::Value::Null),
        ("fields", q::Value::Null),
        (
            "inputFields",
            q::Value::List(vec![
                object_value(vec![
                    ("name", q::Value::String("name_eq".to_string())),
                    ("description", q::Value::Null),
                    (
                        "defaultValue",
                        q::Value::String("\"default name\"".to_string()),
                    ),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("SCALAR".to_string())),
                            ("name", q::Value::String("String".to_string())),
                            ("ofType", q::Value::Null),
                        ]),
                    ),
                ]),
                object_value(vec![
                    ("name", q::Value::String("name_not".to_string())),
                    ("description", q::Value::Null),
                    ("defaultValue", q::Value::Null),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("SCALAR".to_string())),
                            ("name", q::Value::String("String".to_string())),
                            ("ofType", q::Value::Null),
                        ]),
                    ),
                ]),
            ]),
        ),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::Null),
        ("possibleTypes", q::Value::Null),
    ]);

    let user_type = object_value(vec![
        ("kind", q::Value::Enum("OBJECT".to_string())),
        ("name", q::Value::String("User".to_string())),
        ("description", q::Value::Null),
        (
            "fields",
            q::Value::List(vec![
                object_value(vec![
                    ("name", q::Value::String("id".to_string())),
                    ("description", q::Value::Null),
                    ("args", q::Value::List(vec![])),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("NON_NULL".to_string())),
                            ("name", q::Value::Null),
                            (
                                "ofType",
                                object_value(vec![
                                    ("kind", q::Value::Enum("SCALAR".to_string())),
                                    ("name", q::Value::String("ID".to_string())),
                                    ("ofType", q::Value::Null),
                                ]),
                            ),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("name".to_string())),
                    ("description", q::Value::Null),
                    ("args", q::Value::List(vec![])),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("NON_NULL".to_string())),
                            ("name", q::Value::Null),
                            (
                                "ofType",
                                object_value(vec![
                                    ("kind", q::Value::Enum("SCALAR".to_string())),
                                    ("name", q::Value::String("String".to_string())),
                                    ("ofType", q::Value::Null),
                                ]),
                            ),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("role".to_string())),
                    ("description", q::Value::Null),
                    ("args", q::Value::List(vec![])),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("NON_NULL".to_string())),
                            ("name", q::Value::Null),
                            (
                                "ofType",
                                object_value(vec![
                                    ("kind", q::Value::Enum("ENUM".to_string())),
                                    ("name", q::Value::String("Role".to_string())),
                                    ("ofType", q::Value::Null),
                                ]),
                            ),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
            ]),
        ),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        (
            "interfaces",
            q::Value::List(vec![object_value(vec![
                ("kind", q::Value::Enum("INTERFACE".to_string())),
                ("name", q::Value::String("Node".to_string())),
                ("ofType", q::Value::Null),
            ])]),
        ),
        ("possibleTypes", q::Value::Null),
    ]);

    let query_type = object_value(vec![
        ("kind", q::Value::Enum("OBJECT".to_string())),
        ("name", q::Value::String("Query".to_string())),
        ("description", q::Value::Null),
        (
            "fields",
            q::Value::List(vec![
                object_value(vec![
                    ("name", q::Value::String("allUsers".to_string())),
                    ("description", q::Value::Null),
                    (
                        "args",
                        q::Value::List(vec![
                            object_value(vec![
                                ("defaultValue", q::Value::Null),
                                ("description", q::Value::Null),
                                ("name", q::Value::String("orderBy".to_string())),
                                (
                                    "type",
                                    object_value(vec![
                                        ("kind", q::Value::Enum("ENUM".to_string())),
                                        ("name", q::Value::String("User_orderBy".to_string())),
                                        ("ofType", q::Value::Null),
                                    ]),
                                ),
                            ]),
                            object_value(vec![
                                ("defaultValue", q::Value::Null),
                                ("description", q::Value::Null),
                                ("name", q::Value::String("filter".to_string())),
                                (
                                    "type",
                                    object_value(vec![
                                        ("kind", q::Value::Enum("INPUT_OBJECT".to_string())),
                                        ("name", q::Value::String("User_filter".to_string())),
                                        ("ofType", q::Value::Null),
                                    ]),
                                ),
                            ]),
                        ]),
                    ),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("LIST".to_string())),
                            ("name", q::Value::Null),
                            (
                                "ofType",
                                object_value(vec![
                                    ("kind", q::Value::Enum("NON_NULL".to_string())),
                                    ("name", q::Value::Null),
                                    (
                                        "ofType",
                                        object_value(vec![
                                            ("kind", q::Value::Enum("OBJECT".to_string())),
                                            ("name", q::Value::String("User".to_string())),
                                            ("ofType", q::Value::Null),
                                        ]),
                                    ),
                                ]),
                            ),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("anyUserWithAge".to_string())),
                    ("description", q::Value::Null),
                    (
                        "args",
                        q::Value::List(vec![object_value(vec![
                            ("defaultValue", q::Value::String("99".to_string())),
                            ("description", q::Value::Null),
                            ("name", q::Value::String("age".to_string())),
                            (
                                "type",
                                object_value(vec![
                                    ("kind", q::Value::Enum("SCALAR".to_string())),
                                    ("name", q::Value::String("Int".to_string())),
                                    ("ofType", q::Value::Null),
                                ]),
                            ),
                        ])]),
                    ),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("OBJECT".to_string())),
                            ("name", q::Value::String("User".to_string())),
                            ("ofType", q::Value::Null),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
                object_value(vec![
                    ("name", q::Value::String("User".to_string())),
                    ("description", q::Value::Null),
                    ("args", q::Value::List(vec![])),
                    (
                        "type",
                        object_value(vec![
                            ("kind", q::Value::Enum("OBJECT".to_string())),
                            ("name", q::Value::String("User".to_string())),
                            ("ofType", q::Value::Null),
                        ]),
                    ),
                    ("isDeprecated", q::Value::Boolean(false)),
                    ("deprecationReason", q::Value::Null),
                ]),
            ]),
        ),
        ("inputFields", q::Value::Null),
        ("enumValues", q::Value::Null),
        ("interfaces", q::Value::List(vec![])),
        ("possibleTypes", q::Value::Null),
    ]);

    let expected_types = q::Value::List(vec![
        boolean_type,
        id_type,
        int_type,
        node_type,
        query_type,
        role_type,
        string_type,
        user_type,
        user_filter_type,
        user_orderby_type,
    ]);

    let expected_directives = q::Value::List(vec![object_value(vec![
        ("name", q::Value::String("language".to_string())),
        ("description", q::Value::Null),
        (
            "locations",
            q::Value::List(vec![q::Value::Enum(String::from("FIELD_DEFINITION"))]),
        ),
        (
            "args",
            q::Value::List(vec![object_value(vec![
                ("name", q::Value::String("language".to_string())),
                ("description", q::Value::Null),
                ("defaultValue", q::Value::String("\"English\"".to_string())),
                (
                    "type",
                    object_value(vec![
                        ("kind", q::Value::Enum("SCALAR".to_string())),
                        ("name", q::Value::String("String".to_string())),
                        ("ofType", q::Value::Null),
                    ]),
                ),
            ])]),
        ),
    ])]);

    let schema_type = object_value(vec![
        (
            "queryType",
            object_value(vec![("name", q::Value::String("Query".to_string()))]),
        ),
        ("mutationType", q::Value::Null),
        ("subscriptionType", q::Value::Null),
        ("types", expected_types),
        ("directives", expected_directives),
    ]);

    object_value(vec![("__schema", schema_type)])
}

/// Execute an introspection query.
fn introspection_query(schema: Schema, query: &str) -> QueryResult {
    // Create the query
    let query = Query {
        schema: Arc::new(schema),
        document: graphql_parser::parse_query(query).unwrap(),
        variables: None,
    };

    // Execute it
    execute_query(
        &query,
        QueryExecutionOptions {
            logger: Logger::root(slog::Discard, o!()),
            resolver: MockResolver,
            deadline: None,
            max_complexity: None,
            max_depth: 100,
            max_first: std::u32::MAX,
        },
    )
}

#[test]
fn satisfies_graphiql_introspection_query_without_fragments() {
    let result = introspection_query(
        mock_schema(),
        "
      query IntrospectionQuery {
        __schema {
          queryType { name }
          mutationType { name }
          subscriptionType { name}
          types {
            kind
            name
            description
            fields(includeDeprecated: true) {
              name
              description
              args {
                name
                description
                type {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                              ofType {
                                kind
                                name
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
                defaultValue
              }
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              isDeprecated
              deprecationReason
            }
            inputFields {
              name
              description
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              defaultValue
            }
            interfaces {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
            enumValues(includeDeprecated: true) {
              name
              description
              isDeprecated
              deprecationReason
            }
            possibleTypes {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                          }
                        }
                      }
                    }
                  }
                }
              }
           }
          }
          directives {
            name
            description
            locations
            args {
              name
              description
              type {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                        ofType {
                          kind
                          name
                          ofType {
                            kind
                            name
                            ofType {
                              kind
                              name
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
              defaultValue
            }
          }
        }
      }
    ",
    );

    let data = result.data.expect("Introspection query returned no result");
    assert_eq!(data, expected_mock_schema_introspection());
}

#[test]
fn satisfies_graphiql_introspection_query_with_fragments() {
    let result = introspection_query(
        mock_schema(),
        "
      query IntrospectionQuery {
        __schema {
          queryType { name }
          mutationType { name }
          subscriptionType { name }
          types {
            ...FullType
          }
          directives {
            name
            description
            locations
            args {
              ...InputValue
            }
          }
        }
      }

      fragment FullType on __Type {
        kind
        name
        description
        fields(includeDeprecated: true) {
          name
          description
          args {
            ...InputValue
          }
          type {
            ...TypeRef
          }
          isDeprecated
          deprecationReason
        }
        inputFields {
          ...InputValue
        }
        interfaces {
          ...TypeRef
        }
        enumValues(includeDeprecated: true) {
          name
          description
          isDeprecated
          deprecationReason
        }
        possibleTypes {
          ...TypeRef
        }
      }

      fragment InputValue on __InputValue {
        name
        description
        type { ...TypeRef }
        defaultValue
      }

      fragment TypeRef on __Type {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                    }
                  }
                }
              }
            }
          }
        }
      }
    ",
    );

    let data = result.data.expect("Introspection query returned no result");
    assert_eq!(data, expected_mock_schema_introspection());
}

const COMPLEX_SCHEMA: &str = "
enum RegEntryStatus {
  regEntry_status_challengePeriod
  regEntry_status_commitPeriod
  regEntry_status_revealPeriod
  regEntry_status_blacklisted
  regEntry_status_whitelisted
}

interface RegEntry {
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
}

enum VoteOption {
  voteOption_noVote
  voteOption_voteFor
  voteOption_voteAgainst
}

type Vote @entity {
  vote_secretHash: String
  vote_option: VoteOption
  vote_amount: Int
  vote_revealedOn: String
  vote_claimedRewardOn: String
  vote_reward: Int
}

type Meme implements RegEntry @entity {
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
  # Balance of voting token of a voter. This is client-side only, server doesn't return this
  challenge_availableVoteAmount(voter: ID!): Int
  meme_title: String
  meme_number: Int
  meme_metaHash: String
  meme_imageHash: String
  meme_totalSupply: Int
  meme_totalMinted: Int
  meme_tokenIdStart: Int
  meme_totalTradeVolume: Int
  meme_totalTradeVolumeRank: Int
  meme_ownedMemeTokens(owner: String): [MemeToken]
  meme_tags: [Tag]
}

type Tag  @entity {
  tag_id: ID
  tag_name: String
}

type MemeToken @entity {
  memeToken_tokenId: ID
  memeToken_number: Int
  memeToken_owner: User
  memeToken_meme: Meme
}

enum MemeAuctionStatus {
  memeAuction_status_active
  memeAuction_status_canceled
  memeAuction_status_done
}

type MemeAuction @entity {
  memeAuction_address: ID
  memeAuction_seller: User
  memeAuction_buyer: User
  memeAuction_startPrice: Int
  memeAuction_endPrice: Int
  memeAuction_duration: Int
  memeAuction_startedOn: String
  memeAuction_boughtOn: String
  memeAuction_status: MemeAuctionStatus
  memeAuction_memeToken: MemeToken
}

type ParamChange implements RegEntry @entity {
  regEntry_address: ID
  regEntry_version: Int
  regEntry_status: RegEntryStatus
  regEntry_creator: User
  regEntry_deposit: Int
  regEntry_createdOn: String
  regEntry_challengePeriodEnd: String
  challenge_challenger: User
  challenge_createdOn: String
  challenge_comment: String
  challenge_votingToken: String
  challenge_rewardPool: Int
  challenge_commitPeriodEnd: String
  challenge_revealPeriodEnd: String
  challenge_votesFor: Int
  challenge_votesAgainst: Int
  challenge_votesTotal: Int
  challenge_claimedRewardOn: String
  challenge_vote(vote_voter: ID!): Vote
  # Balance of voting token of a voter. This is client-side only, server doesn't return this
  challenge_availableVoteAmount(voter: ID!): Int
  paramChange_db: String
  paramChange_key: String
  paramChange_value: Int
  paramChange_originalValue: Int
  paramChange_appliedOn: String
}

type User @entity {
  # Ethereum address of an user
  user_address: ID
  # Total number of memes submitted by user
  user_totalCreatedMemes: Int
  # Total number of memes submitted by user, which successfully got into TCR
  user_totalCreatedMemesWhitelisted: Int
  # Largest sale creator has done with his newly minted meme
  user_creatorLargestSale: MemeAuction
  # Position of a creator in leaderboard according to user_totalCreatedMemesWhitelisted
  user_creatorRank: Int
  # Amount of meme tokenIds owned by user
  user_totalCollectedTokenIds: Int
  # Amount of unique memes owned by user
  user_totalCollectedMemes: Int
  # Largest auction user sold, in terms of price
  user_largestSale: MemeAuction
  # Largest auction user bought into, in terms of price
  user_largestBuy: MemeAuction
  # Amount of challenges user created
  user_totalCreatedChallenges: Int
  # Amount of challenges user created and ended up in his favor
  user_totalCreatedChallengesSuccess: Int
  # Total amount of DANK token user received from challenger rewards
  user_challengerTotalEarned: Int
  # Total amount of DANK token user received from challenger rewards
  user_challengerRank: Int
  # Amount of different votes user participated in
  user_totalParticipatedVotes: Int
  # Amount of different votes user voted for winning option
  user_totalParticipatedVotesSuccess: Int
  # Amount of DANK token user received for voting for winning option
  user_voterTotalEarned: Int
  # Position of voter in leaderboard according to user_voterTotalEarned
  user_voterRank: Int
  # Sum of user_challengerTotalEarned and user_voterTotalEarned
  user_curatorTotalEarned: Int
  # Position of curator in leaderboard according to user_curatorTotalEarned
  user_curatorRank: Int
}

type Parameter @entity {
  param_db: ID
  param_key: ID
  param_value: Int
}
";

#[test]
fn successfully_runs_introspection_query_against_complex_schema() {
    let mut schema = Schema::parse(
        COMPLEX_SCHEMA,
        SubgraphDeploymentId::new("complexschema").unwrap(),
    )
    .unwrap();
    schema.document = api_schema(&schema.document).unwrap();

    let result = introspection_query(
        schema.clone(),
        "
        query IntrospectionQuery {
          __schema {
            queryType { name }
            mutationType { name }
            subscriptionType { name }
            types {
              ...FullType
            }
            directives {
              name
              description
              locations
              args {
                ...InputValue
              }
            }
          }
        }

        fragment FullType on __Type {
          kind
          name
          description
          fields(includeDeprecated: true) {
            name
            description
            args {
              ...InputValue
            }
            type {
              ...TypeRef
            }
            isDeprecated
            deprecationReason
          }
          inputFields {
            ...InputValue
          }
          interfaces {
            ...TypeRef
          }
          enumValues(includeDeprecated: true) {
            name
            description
            isDeprecated
            deprecationReason
          }
          possibleTypes {
            ...TypeRef
          }
        }

        fragment InputValue on __InputValue {
          name
          description
          type { ...TypeRef }
          defaultValue
        }

        fragment TypeRef on __Type {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType {
                  kind
                  name
                  ofType {
                    kind
                    name
                    ofType {
                      kind
                      name
                      ofType {
                        kind
                        name
                      }
                    }
                  }
                }
              }
            }
          }
        }
    ",
    );

    assert!(result.errors.is_none(), format!("{:#?}", result.errors));
}

#[test]
fn introspection_possible_types() {
    let mut schema = Schema::parse(
        COMPLEX_SCHEMA,
        SubgraphDeploymentId::new("complexschema").unwrap(),
    )
    .unwrap();
    schema.document = api_schema(&schema.document).unwrap();

    // Test "possibleTypes" introspection in interfaces
    let response = introspection_query(
        schema,
        "query {
          __type(name: \"RegEntry\") {
              name
              possibleTypes {
                name
              }
          }
        }",
    )
    .data
    .unwrap();

    assert_eq!(
        response,
        object_value(vec![(
            "__type",
            object_value(vec![
                ("name", q::Value::String("RegEntry".to_string())),
                (
                    "possibleTypes",
                    q::Value::List(vec![
                        object_value(vec![("name", q::Value::String("Meme".to_owned()))]),
                        object_value(vec![("name", q::Value::String("ParamChange".to_owned()))])
                    ])
                )
            ])
        )])
    )
}
