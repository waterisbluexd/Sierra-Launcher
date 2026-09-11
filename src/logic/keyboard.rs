use layer_shika::slint_interpreter::{ComponentInstance, Value};
use layer_shika::calloop::channel;
use std::sync::{Arc, atomic::AtomicU64};
use std::time::Duration;
use slint::ComponentHandle;
use layer_shika::calloop::{TimeoutAction, Timer};
use crate::DaemonMsg;
use crate::cards::wallpaper::WallpaperManager;
use crate::cards::appgrid::AppGrid;
use crate::ui;
use layer_shika::prelude::*;
use layer_shika_adapters::AppState;

const FOCUS_SEARCH_DELAY_MS: u64 = 50;
const ISLAND_NAME: &str = "Island";

pub fn wire_keyboard_callbacks(
    instance: &ComponentInstance,
    manager: &std::rc::Rc<std::cell::RefCell<WallpaperManager>>,
    sender: channel::Sender<DaemonMsg>,
    commit_gen: Arc<AtomicU64>,
    app_grid: std::rc::Rc<AppGrid>,
) {
    let weak = instance.as_weak();
    let manager_prev = manager.clone();
    let sender_prev = sender.clone();
    let commit_gen_prev = commit_gen.clone();

    let _ = instance.set_callback("request_select_prev", move |_args: &[Value]| {
        manager_prev.borrow_mut().select_prev();
        if let Some(inst) = weak.upgrade() {
            ui::push_wallpaper_state(&inst, &manager_prev.borrow());
        }
        {
            let sender_inner = sender_prev.clone();
            ui::kick_loads(&manager_prev, move || {
                let _ = sender_inner.send(DaemonMsg::WallpaperLoaded);
            });
        }
        ui::schedule_commit(&commit_gen_prev, &sender_prev);
        Value::Void
    });

    let weak = instance.as_weak();
    let manager_next = manager.clone();
    let sender_next = sender.clone();
    let commit_gen_next = commit_gen.clone();

    let _ = instance.set_callback("request_select_next", move |_args: &[Value]| {
        manager_next.borrow_mut().select_next();
        if let Some(inst) = weak.upgrade() {
            ui::push_wallpaper_state(&inst, &manager_next.borrow());
        }
        {
            let sender_inner = sender_next.clone();
            ui::kick_loads(&manager_next, move || {
                let _ = sender_inner.send(DaemonMsg::WallpaperLoaded);
            });
        }
        ui::schedule_commit(&commit_gen_next, &sender_next);
        Value::Void
    });

    let sender_hide = sender.clone();

    let _ = instance.set_callback("request_hide", move |_args: &[Value]| {
        let _ = sender_hide.send(DaemonMsg::Toggle);
        Value::Void
    });

    let app_grid_search = app_grid.clone();
    let weak_search = instance.as_weak();

    let _ = instance.set_callback("search_edited", move |args: &[Value]| {
        if let Some(Value::String(text)) = args.first() {
            if let Some(inst) = weak_search.upgrade() {
                crate::cards::appgrid::push_filtered_apps_state(&inst, app_grid_search.as_ref(), text);
            }
        }
        Value::Void
    });

    let app_grid_search2 = app_grid.clone();
    let weak_search2 = instance.as_weak();
    let sender_search_accepted = sender.clone();

    let _ = instance.set_callback("search_accepted", move |args: &[Value]| {
        if let Some(Value::String(text)) = args.first() {
            if let Some(inst) = weak_search2.upgrade() {
                crate::cards::appgrid::push_filtered_apps_state(&inst, app_grid_search2.as_ref(), text);
                if app_grid_search2.launch_first(text) {
                    let _ = sender_search_accepted.send(DaemonMsg::Toggle);
                }
            }
        }
        Value::Void
    });

    let app_grid_launch = app_grid;
    let sender_launch = sender.clone();

    let _ = instance.set_callback("launch-app", move |args: &[Value]| {
        if let Some(Value::String(name)) = args.first() {
            app_grid_launch.launch(name);
            let _ = sender_launch.send(DaemonMsg::Toggle);
        }
        Value::Void
    });
}

pub fn schedule_initial_focus(shell: &mut layer_shika::Shell) {
    let _ = shell.event_loop_handle().insert_source(
        Timer::from_duration(Duration::from_millis(FOCUS_SEARCH_DELAY_MS)),
        move |_deadline, _metadata, app_state: &mut AppState| {
            for surface in app_state.surfaces_by_name_mut(ISLAND_NAME) {
                let _ = surface.component_instance().invoke("focus_search", &[]);
            }
            for surface in app_state.all_outputs() {
                let _ = surface.render_frame_if_dirty();
                surface.commit_surface();
            }
            TimeoutAction::Drop
        },
    );
}
