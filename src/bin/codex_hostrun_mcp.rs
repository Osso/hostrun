use hostrun::mcp_server::run_stdio_server_auto_approve;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_stdio_server_auto_approve().await
}
