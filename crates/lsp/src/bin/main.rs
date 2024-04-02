use lsp::config::Config;
use lsp::from_json;
use lsp::server_capabilities;
use lsp_server::Connection;

fn main() {
    run_server().unwrap();
}

fn run_server() -> anyhow::Result<()> {
    let (connection, io_threads) = Connection::stdio();

    // wait for init request from client
    let (initialize_id, initialize_params) = match connection.initialize_start() {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    let lsp_types::InitializeParams {
        root_uri,
        capabilities,
        workspace_folders,
        initialization_options,
        client_info,
        ..
    } = from_json::<lsp_types::InitializeParams>("InitializeParams", &initialize_params)?;

    // we can later pass the init params to the config
    let config = Config::default();

    let server_capabilities = server_capabilities(&config);

    let initialize_result = lsp_types::InitializeResult {
        capabilities: server_capabilities,
        server_info: Some(lsp_types::ServerInfo {
            name: String::from("postgres_lsp"),
            version: Some("0.0.0".to_string()),
        }),
    };

    let initialize_result = serde_json::to_value(initialize_result).unwrap();

    if let Err(e) = connection.initialize_finish(initialize_id, initialize_result) {
        if e.channel_is_disconnected() {
            io_threads.join()?;
        }
        return Err(e.into());
    }

    main_loop(config, connection)?;

    io_threads.join()?;
    Ok(())
}
