use crate::collection::{Collection, Protocol, RequestDocument};
use crate::grpc::GrpcBody;
use crate::{LynoraError, Result};
use prost_reflect::DescriptorPool;
use std::path::Path;
use uuid::Uuid;

/// Compile a `.proto` file (and imports from its directory) into request stubs.
pub fn import_proto_file(proto_path: &Path, dest_root: &Path, default_endpoint: &str) -> Result<Collection> {
    let proto_src = std::fs::read_to_string(proto_path)?;
    import_proto_source(&proto_src, dest_root, default_endpoint)
}

/// Import from in-memory proto source (desktop file picker).
pub fn import_proto_source(
    proto_src: &str,
    dest_root: &Path,
    default_endpoint: &str,
) -> Result<Collection> {
    std::fs::create_dir_all(dest_root)?;
    let proto_path = dest_root.join("source.proto");
    std::fs::write(&proto_path, proto_src)?;

    let include_dir = dest_root.to_path_buf();
    let file_desc_set = protox::compile([&proto_path], [include_dir.as_path()])
        .map_err(|e| LynoraError::Import(format!("proto compile failed: {e}")))?;

    let pool = DescriptorPool::from_file_descriptor_set(file_desc_set)
        .map_err(|e| LynoraError::Import(format!("descriptor pool: {e}")))?;

    let mut col = Collection::create(dest_root, "proto")?;
    // recreate would wipe - Collection::create already made dirs; rewrite meta name from package if possible
    if let Some(svc) = pool.services().next() {
        let pkg = svc.full_name().split('.').next().unwrap_or("proto");
        col.meta.name = pkg.to_string();
        let _ = std::fs::write(
            dest_root.join("lynora.json"),
            serde_json::to_string_pretty(&col.meta)?,
        );
    }

    for service in pool.services() {
        let service_name = service.full_name().to_string();
        for method in service.methods() {
            let input = method.input();
            let stub = serde_json::json!({});
            let doc = RequestDocument {
                id: Uuid::new_v4().to_string(),
                name: format!("{service_name}/{}", method.name()),
                method: "POST".into(),
                url: default_endpoint.to_string(),
                headers: vec![],
                body: Some(serde_json::to_string_pretty(&stub)?),
                protocol: Protocol::Grpc,
                auth: None,
                graphql: None,
                grpc: Some(GrpcBody {
                    service: service_name.clone(),
                    method: method.name().to_string(),
                    message_json: serde_json::to_string_pretty(&stub)?,
                    proto_file: Some("source.proto".into()),
                    streaming: method.is_client_streaming() || method.is_server_streaming(),
                    input_type: input.full_name().to_string(),
                }),
                expect_status: None,
                websocket: None,
                sse: None,
            };
            col.save_request(&doc)?;
        }
    }

    if col.requests.is_empty() {
        return Err(LynoraError::Import(
            "no gRPC services/methods found in proto".into(),
        ));
    }

    Ok(col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn imports_greeter_methods() {
        let dir = tempdir().unwrap();
        let proto = dir.path().join("hello.proto");
        std::fs::write(
            &proto,
            r#"
syntax = "proto3";
package helloworld;
service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
}
message HelloRequest { string name = 1; }
message HelloReply { string message = 1; }
"#,
        )
        .unwrap();
        let dest = dir.path().join("col");
        let col = import_proto_file(&proto, &dest, "http://127.0.0.1:50051").unwrap();
        assert_eq!(col.requests.len(), 1);
        let req = &col.requests[0];
        assert_eq!(req.protocol, Protocol::Grpc);
        let grpc = req.grpc.as_ref().unwrap();
        assert_eq!(grpc.service, "helloworld.Greeter");
        assert_eq!(grpc.method, "SayHello");
        assert!(dest.join("source.proto").exists());
    }
}
