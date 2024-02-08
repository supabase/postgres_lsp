use std::{env, path::PathBuf};

use lsp_server::Connection;
use postgres_lsp::{config::Config, from_json};
use vfs::AbsPathBuf;

fn main() -> anyhow::Result<()> {
    // postgres_lsp’s “main thread” is actually
    // a secondary latency-sensitive thread with an increased stack size.
    // We use this thread intent because any delay in the main loop
    // will make actions like hitting enter in the editor slow.
    with_extra_thread(
        "LspServer",
        stdx::thread::ThreadIntent::LatencySensitive,
        run_server,
    )?;

    Ok(())
}

const STACK_SIZE: usize = 1024 * 1024 * 8;

/// Parts of postgres_lsp can use a lot of stack space, and some operating systems only give us
/// 1 MB by default (eg. Windows), so this spawns a new thread with hopefully sufficient stack
/// space.
fn with_extra_thread(
    thread_name: impl Into<String>,
    thread_intent: stdx::thread::ThreadIntent,
    f: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    let handle = stdx::thread::Builder::new(thread_intent)
        .name(thread_name.into())
        .stack_size(STACK_SIZE)
        .spawn(f)?;

    handle.join()?;

    Ok(())
}

fn run_server() -> anyhow::Result<()> {
    tracing::info!("server will start");

    let (connection, io_threads) = Connection::stdio();

    let (initialize_id, initialize_params) = match connection.initialize_start() {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    tracing::info!("InitializeParams: {}", initialize_params);

    let lsp_types::InitializeParams {
        root_uri,
        capabilities,
        initialization_options,
        client_info,
        ..
    } = from_json::<lsp_types::InitializeParams>("InitializeParams", &initialize_params)?;

    let root_path = match root_uri
        .and_then(|it| it.to_file_path().ok())
        .map(patch_path_prefix)
        .and_then(|it| AbsPathBuf::try_from(it).ok())
    {
        Some(it) => it,
        None => {
            let cwd = env::current_dir()?;
            AbsPathBuf::assert(cwd)
        }
    };

    let mut is_visual_studio_code = false;
    if let Some(client_info) = client_info {
        tracing::info!(
            "Client '{}' {}",
            client_info.name,
            client_info.version.unwrap_or_default()
        );
        is_visual_studio_code = client_info.name.starts_with("Visual Studio Code");
    }

    let mut config = Config::new(root_path, capabilities, is_visual_studio_code);
    if let Some(json) = initialization_options {
        if let Err(e) = config.update(json) {
            use lsp_types::{
                notification::{Notification, ShowMessage},
                MessageType, ShowMessageParams,
            };
            let not = lsp_server::Notification::new(
                ShowMessage::METHOD.to_string(),
                ShowMessageParams {
                    typ: MessageType::WARNING,
                    message: e.to_string(),
                },
            );
            connection
                .sender
                .send(lsp_server::Message::Notification(not))
                .unwrap();
        }
    }

    let server_capabilities = postgres_lsp::server_capabilities(&config);

    let initialize_result = lsp_types::InitializeResult {
        capabilities: server_capabilities,
        server_info: Some(lsp_types::ServerInfo {
            name: String::from("postgres_lsp"),
            version: None,
        }),
        offset_encoding: None,
    };

    let initialize_result = serde_json::to_value(initialize_result).unwrap();

    if let Err(e) = connection.initialize_finish(initialize_id, initialize_result) {
        if e.channel_is_disconnected() {
            io_threads.join()?;
        }
        return Err(e.into());
    }

    postgres_lsp::main_loop(config, connection)?;

    io_threads.join()?;
    tracing::info!("server did shut down");

    Ok(())
}

fn patch_path_prefix(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};
    if cfg!(windows) {
        // VSCode might report paths with the file drive in lowercase, but this can mess
        // with env vars set by tools and build scripts executed by pg_lsp such that it invalidates
        // cargo's compilations unnecessarily. https://github.com/rust-lang/rust-analyzer/issues/14683
        // So we just uppercase the drive letter here unconditionally.
        // (doing it conditionally is a pain because std::path::Prefix always reports uppercase letters on windows)
        let mut comps = path.components();
        match comps.next() {
            Some(Component::Prefix(prefix)) => {
                let prefix = match prefix.kind() {
                    Prefix::Disk(d) => {
                        format!("{}:", d.to_ascii_uppercase() as char)
                    }
                    Prefix::VerbatimDisk(d) => {
                        format!(r"\\?\{}:", d.to_ascii_uppercase() as char)
                    }
                    _ => return path,
                };
                let mut path = PathBuf::new();
                path.push(prefix);
                path.extend(comps);
                path
            }
            _ => path,
        }
    } else {
        path
    }
}

#[test]
#[cfg(windows)]
fn patch_path_prefix_works() {
    assert_eq!(
        patch_path_prefix(r"c:\foo\bar".into()),
        PathBuf::from(r"C:\foo\bar")
    );
    assert_eq!(
        patch_path_prefix(r"\\?\c:\foo\bar".into()),
        PathBuf::from(r"\\?\C:\foo\bar")
    );
}
