use crate::rest::RestResponse;
use crate::{LynoraError, Result};
use bytes::{Buf, Bytes};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, SerializeOptions};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::Channel;
use tonic::{Code, Request, Status};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GrpcBody {
    pub service: String,
    pub method: String,
    pub message_json: String,
    /// Relative path inside the collection root, or absolute path.
    #[serde(default)]
    pub proto_file: Option<String>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub input_type: String,
}

#[derive(Debug, Clone)]
pub struct GrpcRequest {
    pub endpoint: String,
    pub body: GrpcBody,
    pub collection_root: Option<std::path::PathBuf>,
    pub headers: Vec<(String, String)>,
}

fn resolve_proto_path(req: &GrpcRequest) -> Result<std::path::PathBuf> {
    let rel = req
        .body
        .proto_file
        .as_deref()
        .ok_or_else(|| LynoraError::Message("gRPC request missing proto_file".into()))?;
    let path = Path::new(rel);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let root = req
        .collection_root
        .as_ref()
        .ok_or_else(|| LynoraError::Message("collection_root required for relative proto".into()))?;
    Ok(root.join(path))
}

fn load_pool(proto_path: &Path) -> Result<DescriptorPool> {
    let include = proto_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    let fds = protox::compile([proto_path], [include.as_path()])
        .map_err(|e| LynoraError::Message(format!("proto compile: {e}")))?;
    DescriptorPool::from_file_descriptor_set(fds)
        .map_err(|e| LynoraError::Message(format!("descriptor pool: {e}")))
}

struct DualDynamicCodec {
    encode_type: prost_reflect::MessageDescriptor,
    decode_type: prost_reflect::MessageDescriptor,
}

#[derive(Clone)]
struct DynamicEncoder {
    message: prost_reflect::MessageDescriptor,
}

#[derive(Clone)]
struct DynamicDecoder {
    message: prost_reflect::MessageDescriptor,
}

impl Codec for DualDynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder {
            message: self.encode_type.clone(),
        }
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            message: self.decode_type.clone(),
        }
    }
}

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut EncodeBuf<'_>,
    ) -> std::result::Result<(), Self::Error> {
        let _ = &self.message;
        item.encode(dst)
            .map_err(|e| Status::new(Code::Internal, e.to_string()))
    }
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(
        &mut self,
        src: &mut DecodeBuf<'_>,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        if src.remaining() == 0 {
            return Ok(None);
        }
        let buf: Bytes = src.copy_to_bytes(src.remaining());
        let msg = DynamicMessage::decode(self.message.clone(), buf.as_ref())
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(Some(msg))
    }
}

/// Unary gRPC call. Streaming methods error with a clear message in 0.3.
pub async fn send_unary(req: GrpcRequest) -> Result<RestResponse> {
    if req.body.streaming {
        return Err(LynoraError::Message(
            "streaming gRPC UI is basic in 0.3 — use unary methods for now".into(),
        ));
    }

    let proto_path = resolve_proto_path(&req)?;
    let pool = load_pool(&proto_path)?;
    let service = pool
        .get_service_by_name(&req.body.service)
        .ok_or_else(|| LynoraError::Message(format!("unknown service {}", req.body.service)))?;
    let method = service
        .methods()
        .find(|m| m.name() == req.body.method)
        .ok_or_else(|| LynoraError::Message(format!("unknown method {}", req.body.method)))?;

    let input_desc = method.input();
    let output_desc = method.output();
    let json_value: serde_json::Value = serde_json::from_str(&req.body.message_json)?;
    let input = DynamicMessage::deserialize(input_desc.clone(), json_value)
        .map_err(|e| LynoraError::Message(format!("deserialize request: {e}")))?;

    let endpoint = req.endpoint.trim_end_matches('/').to_string();
    let channel = Channel::from_shared(endpoint)
        .map_err(|e| LynoraError::Message(format!("invalid endpoint: {e}")))?
        .connect()
        .await
        .map_err(|e| LynoraError::Message(format!("gRPC connect failed: {e}")))?;

    let path = http::uri::PathAndQuery::from_maybe_shared(format!(
        "/{}/{}",
        req.body.service.trim_start_matches('/'),
        req.body.method
    ))
    .map_err(|e| LynoraError::Message(format!("invalid grpc path: {e}")))?;

    let mut grpc = Grpc::new(channel);
    grpc.ready()
        .await
        .map_err(|e| LynoraError::Message(format!("gRPC not ready: {e}")))?;

    let codec = DualDynamicCodec {
        encode_type: input_desc,
        decode_type: output_desc,
    };

    let mut request = Request::new(input);
    for (k, v) in &req.headers {
        if let (Ok(key), Ok(val)) = (
            k.parse::<tonic::metadata::MetadataKey<_>>(),
            v.parse::<tonic::metadata::MetadataValue<_>>(),
        ) {
            request.metadata_mut().insert(key, val);
        }
    }

    let started = Instant::now();
    let response = grpc
        .unary(request, path, codec)
        .await
        .map_err(|e| LynoraError::Message(format!("gRPC call failed: {e}")))?;
    let duration_ms = started.elapsed().as_millis();
    let msg = response.into_inner();
    let body = dynamic_to_json(&msg)?;

    Ok(RestResponse {
        status: 0,
        headers: vec![("grpc-status".into(), "0".into())],
        body,
        duration_ms,
    })
}

fn dynamic_to_json(msg: &DynamicMessage) -> Result<String> {
    let options = SerializeOptions::new()
        .stringify_64_bit_integers(false)
        .use_proto_field_name(true);
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::pretty(&mut buf);
    msg.serialize_with_options(&mut ser, &options)
        .map_err(|e| LynoraError::Message(format!("serialize response: {e}")))?;
    Ok(String::from_utf8(buf).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolves_relative_proto() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("source.proto"), "syntax=\"proto3\";").unwrap();
        let req = GrpcRequest {
            endpoint: "http://127.0.0.1:50051".into(),
            body: GrpcBody {
                service: "a.B".into(),
                method: "C".into(),
                message_json: "{}".into(),
                proto_file: Some("source.proto".into()),
                streaming: false,
                input_type: String::new(),
            },
            collection_root: Some(dir.path().to_path_buf()),
            headers: vec![],
        };
        let path = resolve_proto_path(&req).unwrap();
        assert!(path.ends_with("source.proto"));
    }
}
