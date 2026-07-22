//! Lynora core — collections, environments, protocols, import, and history.

pub mod auth;
pub mod codegen;
pub mod collection;
pub mod environment;
pub mod error;
pub mod graphql;
pub mod grpc;
pub mod history;
pub mod import;
pub mod rest;
pub mod vars;
pub mod workspace;

pub use auth::{
    apply_auth_headers, build_authorize_url, exchange_token, expand_auth, generate_pkce,
    ApiKeyLocation, AuthConfig, AuthKind, PkceChallenge,
};
pub use codegen::{generate as generate_code, CodeLanguage};
pub use collection::{Collection, CollectionMeta, Header, Protocol, RequestDocument};
pub use environment::Environment;
pub use error::{LynoraError, Result};
pub use graphql::{
    introspect as introspect_graphql, send as send_graphql, GraphQlBody, GraphQlRequest,
};
pub use grpc::{send_unary as send_grpc, GrpcBody, GrpcRequest};
pub use history::{HistoryEntry, HistoryStore, NewHistoryEntry};
pub use import::openapi::import_openapi_json;
pub use import::postman::import_postman_json;
pub use import::proto::{import_proto_file, import_proto_source};
pub use rest::{prepare_request, send as send_rest, RestRequest, RestResponse};
pub use vars::expand;
pub use workspace::Workspace;
