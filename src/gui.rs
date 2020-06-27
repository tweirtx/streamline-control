use druid::commands::{CLOSE_WINDOW, QUIT_APP};
use druid::widget::{Button, Flex, Label};
use druid::{
    AppDelegate, AppLauncher, Command, Data, DelegateCtx, Env, ExtEventSink, Lens, Selector,
    Target, Widget, WidgetExt, WindowDesc, WindowId,
};

use webbrowser;

use crate::update::{do_update, fetch_is_new, ReleaseStatus};
use crate::server::start_server;

use std::thread;
use tokio::sync::oneshot::Receiver;

const START_UPDATE_CHECK: Selector = Selector::new("streamline-control.start-check");
const UPDATE_FOUND: Selector<String> = Selector::new("streamline-control.update-found");
const NO_UPDATE: Selector = Selector::new("streamline-control.no-update-found");
const START_DO_UPDATE: Selector = Selector::new("streamline-control.do-updates");
const UPDATE_FINISHED: Selector = Selector::new("streamline-control.update-finished");
const UPDATE_ERROR: Selector<String> = Selector::new("streamline-control.update-error");
const OPEN_QUIT_CONFIRM: Selector = Selector::new("streamline-control.quit-confirm-open");

pub fn run_ui() {
    let main_window_id = WindowId::next();
    let mut main_window = WindowDesc::new(ui_builder)
        .window_size((300.0, 160.0))
        .title("Streamline Server Control");
    main_window.id = main_window_id;

    let inital_state = GUIState {
        status: "Server Not Running".into(),
        feedback: "".into(),
        found_update: false,
        update_button: "Check for Updates".into(),
        url: None,
    };

    let app = AppLauncher::with_window(main_window).use_simple_logger();

    let delegate = Delegate {
        eventsink: app.get_external_handle(),
        main_window: main_window_id,
        shutdown_signal: None,
    };

    thread::spawn(start_server);

    app.delegate(delegate)
        .launch(inital_state)
        .expect("Launch failed");
}

#[derive(Clone, Data, Lens)]
struct GUIState {
    status: String,
    feedback: String,
    found_update: bool,
    update_button: String,
    url: Option<String>,
}

fn ui_builder() -> impl Widget<GUIState> {
    let status_label =
        Label::new(|data: &GUIState, _env: &Env| format!("{}", data.status)).padding(5.0);

    let feedback_label = Label::new(|data: &GUIState, _env: &Env| format!("{}", data.feedback));

    let quit_button =
        Button::new("Quit")
            .padding(5.0)
            .on_click(|ctx, _data: &mut GUIState, _env| {
                let cmd = Command::new(QUIT_APP, ());
                ctx.submit_command(cmd, None);
            });

    let check_button = Button::new(|data: &GUIState, _env: &Env| format!("{}", data.update_button))
        .on_click(|ctx, data: &mut GUIState, _env| {
            if data.found_update {
                let cmd = Command::new(START_DO_UPDATE, ());
                ctx.submit_command(cmd, None);
            } else {
                let cmd = Command::new(START_UPDATE_CHECK, ());
                ctx.submit_command(cmd, None);
            }
        })
        .padding(5.0);

    let open_button = Button::new("Open Browser")
        .on_click(move |_ctx, data: &mut GUIState, _env| match &data.url {
            Some(url) => {
                if webbrowser::open(url.as_str()).is_err() == true {
                    data.feedback = "Unable to Open Browser".into();
                }
            }
            None => data.feedback = "No URL yet set".into(),
        })
        .padding(5.0);

    Flex::column()
        .with_child(status_label)
        .with_child(feedback_label)
        .with_spacer(10.0)
        .with_child(open_button)
        .with_child(check_button)
        .with_child(quit_button)
}

fn quit_confirm_ui() -> impl Widget<GUIState> {
    Flex::column()
}

struct Delegate {
    eventsink: ExtEventSink,
    main_window: WindowId,
    shutdown_signal: Option<Receiver<()>>
}

impl AppDelegate<GUIState> for Delegate {
    fn command(
        &mut self,
        ctx: &mut DelegateCtx,
        target: Target,
        cmd: &Command,
        data: &mut GUIState,
        _env: &Env,
    ) -> bool {
        println!("{:?}, {:?}", cmd, target);
        if cmd.is(START_UPDATE_CHECK) {
            data.feedback = "Checking For Updates...".into();
            check_updates(self.eventsink.clone())
        } else if cmd.is(NO_UPDATE) {
            data.feedback = "No Update Found".into();
        } else if let Some(version) = cmd.get(UPDATE_FOUND) {
            data.feedback = format!("New Version Found: {}", version);
            data.found_update = true;
            data.update_button = format!("Update to {}", version);
        } else if let Some(err) = cmd.get(UPDATE_ERROR) {
            data.feedback = format!("Error when checking updates: {}", err);
        } else if cmd.is(START_DO_UPDATE) {
            data.feedback = "Updating App...".into();
            wrapped_do_update(self.eventsink.clone())
        } else if cmd.is(UPDATE_FINISHED) {
            data.feedback = "Update Finished. Please restart the app. ".into();
        } else if cmd.is(OPEN_QUIT_CONFIRM) {
            let quit_confirm_window = WindowDesc::new(quit_confirm_ui)
                .window_size((150.0, 125.0))
                .title("Confirm Quitting Streamline Control");
            ctx.new_window(quit_confirm_window)
        } else if cmd.is(CLOSE_WINDOW) {
            // TODO: This doesn't work. Try a workaround later.
            if Target::Window(self.main_window) == target {
                let new_cmd = Command::new(OPEN_QUIT_CONFIRM, ());
                ctx.submit_command(new_cmd, None);
                return false;
            }
        }
        true
    }
}

fn check_updates(sink: ExtEventSink) {
    thread::spawn(move || {
        let up_to_date = fetch_is_new();
        match up_to_date {
            Ok(ReleaseStatus::UpToDate) => sink.submit_command(NO_UPDATE, (), None),
            Ok(ReleaseStatus::NewVersion(release)) => {
                sink.submit_command(UPDATE_FOUND, String::from(release.version), None)
            }
            Err(err) => sink.submit_command(UPDATE_ERROR, err.to_string(), None),
        }
    });
}

fn wrapped_do_update(sink: ExtEventSink) {
    thread::spawn(move || {
        let has_updated = do_update();
        match has_updated {
            Ok(()) => sink.submit_command(UPDATE_FINISHED, (), None),
            Err(err) => sink.submit_command(UPDATE_ERROR, err.to_string(), None),
        }
    });
}
