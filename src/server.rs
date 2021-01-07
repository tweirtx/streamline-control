use warp::{Filter, path, Reply, Rejection, http::Uri, path::Tail};
use druid::{ExtEventSink, Target};
use tokio::sync::oneshot::Receiver;
use crate::gui::{SERVER_START, UPDATE_STATUS};
use port_scanner::local_ports_available;
use warp::http::{Response, header::HeaderValue};
use rust_embed::RustEmbed;
use app_dirs2::{app_root, AppDataType};
use rusqlite::Connection;
use crate::APP_INFO;
use log::{error, debug};

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!();
}

fn publish_error(error: String, sink: ExtEventSink){
    error!("{}", error);
    sink.submit_command(UPDATE_STATUS, error, Target::Auto)
        .expect("Error sending GUI update");

}

#[tokio::main]
pub async fn start_server(sink: ExtEventSink, rx: Receiver<()>) {

    let mut db_url = match app_root(AppDataType::UserConfig, &APP_INFO) {
        Ok(db) => {
            debug!("Location of user data: {:#?}",db);
            db
        }
        Err(error) => {
            publish_error(error.to_string(), sink);
            return
        }
    };
    db_url.push("streamline.db");

    let mut db_config = match Connection::open(db_url){
        Ok(db) => {db}
        Err(error) => {
            publish_error(error.to_string(), sink);
            return
        }
    };

    match embedded::migrations::runner().run(&mut db_config) {
        Ok(_) => {}
        Err(error) => {
            publish_error(error.to_string(), sink);
            return
        }
    }

    let static_route = path("static").and(path::tail()).and_then(static_serve);
    let dist_route = path("dist").and(path::tail()).and_then(dist_serve);

    let hello = warp::path!("hello" / String)
        .map(|name| format!("Hello, {}!", name));

    let index = warp::path::end()
        .map(|| {
            warp::redirect(Uri::from_static("/app"))
        });

    let app = warp::path("app").and_then(serve_index);

    let routes = static_route.or(dist_route).or(hello).or(index).or(app);

    let mut ports: Vec<u16> = local_ports_available(vec![3030,8888,8080,80]);

    let port = match ports.pop() {
        Some(num) => {num}
        None => {
            publish_error("No open ports to bind to!".to_string(), sink);
            return;
        }
    };

    let server_result = warp::serve(routes)
        .try_bind_with_graceful_shutdown(
            ([127, 0, 0, 1], port),
            async { rx.await.ok();}
        );

    let server_handle = match server_result {
        Ok((addr, future)) => {
            sink.submit_command(SERVER_START, addr, Target::Auto)
                .expect("Error sending GUI update");
            tokio::task::spawn(future)
        }
        Err(error) => {
            publish_error(error.to_string(), sink);
            return;
        }
    };

    server_handle.await.expect("Error starting server thread");

}

#[derive(RustEmbed)]
#[folder = "static/"]
struct Asset;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Dist;

async fn dist_serve(path: Tail) -> Result<impl Reply, Rejection> {
    let asset = Dist::get(path.as_str()).ok_or_else(warp::reject::not_found)?;
    let mime = mime_guess::from_path(path.as_str()).first_or_octet_stream();

    let mut res: Response<std::borrow::Cow<'_, [u8]>> = Response::new(asset.into());
    res.headers_mut().insert("content-type", HeaderValue::from_str(mime.as_ref()).unwrap());
    Ok(res)
}

async fn serve_index() -> Result<impl Reply, Rejection> {
    serve_impl("index.html")
}

async fn static_serve(path: Tail) -> Result<impl Reply, Rejection> {
    serve_impl(path.as_str())
}

fn serve_impl(path: &str) -> Result<impl Reply, Rejection> {
    let asset = Asset::get(path).ok_or_else(warp::reject::not_found)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    let mut res: Response<std::borrow::Cow<'_, [u8]>> = Response::new(asset.into());
    res.headers_mut().insert("content-type", HeaderValue::from_str(mime.as_ref()).unwrap());
    Ok(res)
}
