mod locale;
mod state;
mod view;

use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use winisland_plugin_api::sdk::{
    Activity, CallbackResource, Error, Event, Host, Resource, Rgba, Surface, TextStyle,
};
use winisland_plugin_api::*;

use crate::geometry;

use self::state::{Action, Phase, State};

pub const ID: &str = "winisland-timer";
pub const PAGE_KEY: &str = "timer";
pub const COMPACT_KEY: &str = "timer-compact";
pub const START_COMMAND: &str = "winisland-timer.start";

const PAGE_WIDTH: f32 = 360.0;
const PAGE_HEIGHT: f32 = 200.0;
const COMPACT_WIDTH: f32 = 220.0;
const COMPACT_HEIGHT: f32 = 36.0;
const COMPACT_FONT_SIZE: f32 = 13.0;
const COMPACT_ICON: f32 = 18.0;
const COMPACT_EDGE: f32 = 9.0;
const COMPACT_TEXT_GAP: f32 = 16.0;
const WHEEL_PIXEL_STEP: f32 = 20.0;
const TICK_MS: u64 = 100;

pub fn descriptor() -> PluginDescriptorV2 {
    PluginDescriptorV2 {
        struct_size: std::mem::size_of::<PluginDescriptorV2>() as u32,
        abi_version: ABI_VERSION_2,
        capabilities: CAP_SURFACE
            | CAP_INPUT
            | CAP_EVENTS
            | CAP_TEXT
            | CAP_SYSTEM
            | CAP_ACTIVITY
            | CAP_NOTIFY
            | CAP_I18N
            | CAP_COMMAND,
        metadata: PluginMetadataC::new(
            ID,
            "Timer",
            env!("CARGO_PKG_VERSION"),
            "WinIsland",
            "Countdown timer with presets, fine adjustment, and a finished alert.",
        ),
        create: Some(create),
        shutdown: Some(shutdown),
        destroy: Some(destroy),
        on_tick: Some(tick),
    }
}

struct Target {
    host: Host,
    page: Surface,
    compact: Surface,
    activity: Mutex<Activity>,
}

struct Timer {
    target: Option<Arc<Target>>,
    state: Arc<Mutex<State>>,
    callbacks: Vec<CallbackResource>,
    translations: Vec<Resource>,
}

impl Timer {
    fn new(host: Host) -> Result<Self, Error> {
        let language = host
            .system()
            .and_then(|api| api.current_language())
            .unwrap_or_else(|_| String::from("en_us"));
        let text = locale::text(&language);
        let page = host.surfaces()?.create(page_spec(text.title))?;
        let compact = host.surfaces()?.create(compact_spec())?;
        let page_id = page.id();
        let activity = host.activities()?.create(ActivitySpecV2 {
            flags: ACTIVITY_KEEP_VISIBLE | ACTIVITY_SECONDARY,
            priority: PRIORITY_MEDIUM,
            compact_surface: compact.id(),
            expanded_page: page_id,
            ..Default::default()
        })?;
        let target = Arc::new(Target {
            host: host.clone(),
            page,
            compact,
            activity: Mutex::new(activity),
        });
        let mut state = State::new();
        state.language = language;
        let state = Arc::new(Mutex::new(state));
        let mut translations = Vec::with_capacity(locale::LANGUAGES.len());
        if let Ok(i18n) = host.i18n() {
            for language in locale::LANGUAGES {
                let pairs = locale::pairs(language);
                match i18n.register_bundle(language, &pairs) {
                    Ok(resource) => translations.push(resource),
                    Err(error) => {
                        host.log().write(
                            2,
                            &format!("Timer translations unavailable for {language}: {error}"),
                        );
                    }
                }
            }
        }
        let events = host.events()?;
        let input_target = target.clone();
        let input_state = state.clone();
        let input = events.subscribe(EVENT_INPUT, page_id, move |event| {
            handle_input(&input_target, &input_state, &event);
        })?;
        let visibility_state = state.clone();
        let visibility =
            events.subscribe(EVENT_VISIBILITY | EVENT_RESIZE, page_id, move |event| {
                handle_visibility(&visibility_state, &event);
            })?;
        let tick_target = target.clone();
        let tick_state = state.clone();
        let timer = events.timer(
            TimerSpecV2 {
                target: page_id,
                delay_ms: TICK_MS,
                interval_ms: TICK_MS,
                ..Default::default()
            },
            move |_| {
                if let Err(error) = poll(&tick_target, &tick_state) {
                    tick_target
                        .host
                        .log()
                        .write(3, &format!("Timer poll failed: {error}"));
                }
            },
        )?;
        let command_target = target.clone();
        let command_state = state.clone();
        let command = host.commands()?.register(
            CommandSpecV2 {
                flags: COMMAND_ENABLED,
                key: str_to_fixed("start"),
                title: str_to_fixed("Start timer"),
                ..Default::default()
            },
            move |event| {
                let Ok(value) = std::str::from_utf8(&event.data) else {
                    return;
                };
                let Ok(seconds) = value.trim().parse::<u64>() else {
                    return;
                };
                let Ok(mut state) = command_state.lock() else {
                    return;
                };
                state::start_for(&mut state, seconds.min(24 * 3600));
                if state.alert_open {
                    if let Ok(notification) = command_target.host.notification() {
                        let _ = notification.dismiss_alert();
                    }
                    state.alert_open = false;
                }
                publish(&command_target, &mut state);
            },
        )?;
        Ok(Self {
            target: Some(target),
            state,
            callbacks: vec![input, visibility, timer, command],
            translations,
        })
    }

    fn draw(&self, widget: WidgetId) -> Result<(), Error> {
        let Some(target) = &self.target else {
            return Err(Error::StaleHandle);
        };
        if widget == target.page.id() {
            draw_page(target, &self.state)
        } else if widget == target.compact.id() {
            draw_compact(target, &self.state)
        } else {
            Err(Error::StaleHandle)
        }
    }
}

fn page_spec(title: &str) -> SurfaceSpecV2 {
    SurfaceSpecV2 {
        kind: SURFACE_PAGE,
        flags: SURFACE_ENABLED,
        order: i32::MIN,
        width: PAGE_WIDTH,
        height: PAGE_HEIGHT,
        key: str_to_fixed(PAGE_KEY),
        title: str_to_fixed(title),
        ..Default::default()
    }
}

fn compact_spec() -> SurfaceSpecV2 {
    SurfaceSpecV2 {
        kind: SURFACE_COMPACT_MAIN,
        flags: SURFACE_ENABLED,
        width: COMPACT_WIDTH,
        height: COMPACT_HEIGHT,
        key: str_to_fixed(COMPACT_KEY),
        title: str_to_fixed("Timer"),
        ..Default::default()
    }
}

fn activity_spec(target: &Target, state: &State) -> ActivitySpecV2 {
    let active = state::is_running_or_paused(state.phase);
    ActivitySpecV2 {
        flags: if active {
            ACTIVITY_ENABLED | ACTIVITY_KEEP_VISIBLE | ACTIVITY_SECONDARY
        } else {
            ACTIVITY_KEEP_VISIBLE | ACTIVITY_SECONDARY
        },
        priority: PRIORITY_MEDIUM,
        compact_surface: target.compact.id(),
        expanded_page: target.page.id(),
        preferred_width: state.preferred_width,
        ..Default::default()
    }
}

fn time_width(host: &Host, text: &str, size: f32) -> f32 {
    let Ok(api) = host.text() else {
        return 0.0;
    };
    let mut style = TextStyle::default_at(size);
    style.weight = 600;
    let digit = api.measure_style("0", &style).map_or(0.0, |m| m.width);
    let mut total = 0.0;
    for character in text.chars() {
        total += if character.is_ascii_digit() {
            digit
        } else {
            api.measure_style(&character.to_string(), &style)
                .map_or(0.0, |metrics| metrics.width)
                .max(size * 0.25)
        };
    }
    total
}

fn compact_width(host: &Host, text: &str) -> f32 {
    COMPACT_EDGE * 2.0 + COMPACT_ICON + COMPACT_TEXT_GAP + time_width(host, text, COMPACT_FONT_SIZE)
}

/// Updates the activity spec, its countdown, and the compact width.
fn publish(target: &Target, state: &mut State) {
    let text =
        state::format_remaining(state::remaining(state.phase, state.seconds, Instant::now()));
    state.preferred_width = compact_width(&target.host, &text);
    let activity = match target.activity.lock() {
        Ok(activity) => activity,
        Err(_) => return,
    };
    if let Err(error) = activity.update(activity_spec(target, state)) {
        target
            .host
            .log()
            .write(3, &format!("Timer activity update failed: {error}"));
        return;
    }
    let Ok(indicator) = target.host.indicator() else {
        return;
    };
    if state::is_running_or_paused(state.phase) {
        let now = Instant::now();
        let remaining = state::remaining(state.phase, state.seconds, now);
        let remaining_ms = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
        let paused = matches!(state.phase, Phase::Paused { .. });
        if let Err(error) = indicator.set(&activity, remaining_ms, state.seconds * 1000, paused) {
            target
                .host
                .log()
                .write(3, &format!("Timer indicator update failed: {error}"));
        }
    } else if let Err(error) = indicator.clear(&activity) {
        target
            .host
            .log()
            .write(3, &format!("Timer indicator clear failed: {error}"));
    }
}

fn handle_visibility(state: &Mutex<State>, event: &Event) {
    if event.kind != EVENT_VISIBILITY {
        return;
    }
    let Ok(mut state) = state.lock() else {
        return;
    };
    if event.code == 0 {
        state.selected = None;
        state.hover_target = None;
    }
}

fn region_action(state: &State, sequence: u64) -> Option<Action> {
    state
        .regions
        .get(usize::try_from(sequence).ok()?.checked_sub(1)?)
        .copied()
}

fn handle_input(target: &Target, state: &Mutex<State>, event: &Event) {
    match event.detail {
        INPUT_ENTER | INPUT_MOVE => {
            let Ok(mut state) = state.lock() else {
                return;
            };
            state.hover_target = region_action(&state, event.sequence);
        }
        INPUT_LEAVE | INPUT_CANCEL | INPUT_BLUR => {
            let Ok(mut state) = state.lock() else {
                return;
            };
            state.hover_target = None;
        }
        INPUT_DOWN if event.code == 1 => {
            let Ok(mut state) = state.lock() else {
                return;
            };
            let Some(action) = region_action(&state, event.sequence) else {
                return;
            };
            if state.alert_open {
                if let Ok(notification) = target.host.notification() {
                    let _ = notification.dismiss_alert();
                }
                state.alert_open = false;
            }
            state.selected = None;
            state::apply_action(&mut state, action);
            publish(target, &mut state);
        }
        INPUT_WHEEL => {
            let Ok(mut state) = state.lock() else {
                return;
            };
            let Some(action) = region_action(&state, event.sequence) else {
                return;
            };
            if !matches!(action, Action::Dial | Action::Field(_)) {
                return;
            }
            let dominant = if event.delta.0.abs() > event.delta.1.abs() {
                event.delta.0
            } else {
                -event.delta.1
            };
            let steps = if event.code == 1 {
                state.wheel += dominant;
                let steps = (state.wheel / WHEEL_PIXEL_STEP).trunc();
                state.wheel -= steps * WHEEL_PIXEL_STEP;
                -steps
            } else {
                state.wheel = 0.0;
                -dominant.signum()
            };
            let steps = i32::try_from(steps.clamp(-32.0, 32.0) as i64).unwrap_or(0);
            if state::scroll(&mut state, action, steps) {
                publish(target, &mut state);
            }
        }
        _ => {}
    }
}

/// Detects a finished countdown and reflects it on the island or in a toast.
fn poll(target: &Target, state: &Mutex<State>) -> Result<(), Error> {
    let mut state = state.lock().map_err(|_| Error::Internal)?;
    if let Ok(language) = target
        .host
        .system()
        .and_then(|system| system.current_language())
        && language != state.language
    {
        state.language = language;
        let text = locale::text(&state.language);
        let _ = target.page.update(page_spec(text.title));
    }
    if state.alert_open {
        let visible = target
            .host
            .notification()
            .and_then(|api| api.alert_visible())
            .unwrap_or(false);
        if !visible {
            state.alert_open = false;
            state.phase = Phase::Idle;
        }
    }
    let Some(seconds) = state::poll_finished(&mut state) else {
        return Ok(());
    };
    let text = locale::text(&state.language);
    let label = state::duration_label(seconds, text);
    let body = locale::fill(text.done_toast, &label);
    publish(target, &mut state);
    state.alert_open = false;
    if let Ok(commands) = target.host.commands() {
        let _ = commands.execute("winisland.collapse", &[]);
    }
    let notification = target.host.notification()?;
    notification.alert(text.done, &body, locale::alarm_icon_path())?;
    state.alert_open = true;
    Ok(())
}

fn draw_page(target: &Target, state: &Arc<Mutex<State>>) -> Result<(), Error> {
    let (width, height) = target.page.logical_size();
    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }
    let clock = target
        .host
        .system()
        .and_then(|api| api.local_datetime())
        .map_or((0, 0, 0), |now| (now.hour, now.minute, now.second));
    let mut state = state.lock().map_err(|_| Error::Internal)?;
    set_regions(target, &mut state, width, height)?;
    let mut view = view::View::new(&target.host, width, height);
    view::draw_page(
        &mut view,
        &mut state,
        width,
        height,
        1.0,
        Rgba::WHITE,
        clock,
    );
    let view = view.finish()?;
    target.page.submit(view.finish())
}

fn draw_compact(target: &Target, state: &Arc<Mutex<State>>) -> Result<(), Error> {
    let (width, height) = target.compact.logical_size();
    if width <= 0.0 || height <= 0.0 {
        return Ok(());
    }
    let mut state = state.lock().map_err(|_| Error::Internal)?;
    let mut view = view::View::new(&target.host, width, height);
    view::draw_compact(&mut view, &mut state, width, height, 1.0);
    let view = view.finish()?;
    let result = target.compact.submit(view.finish());
    let text =
        state::format_remaining(state::remaining(state.phase, state.seconds, Instant::now()));
    let preferred = compact_width(&target.host, &text);
    if (preferred - state.preferred_width).abs() >= 1.0 {
        state.preferred_width = preferred;
        if let Ok(activity) = target.activity.lock() {
            let _ = activity.update(activity_spec(target, &state));
        }
    }
    result
}

fn set_regions(target: &Target, state: &mut State, width: f32, height: f32) -> Result<(), Error> {
    let layout = state::layout(width, height);
    let mut regions: Vec<InputRegionV2> = Vec::with_capacity(32);
    let mut actions: Vec<Action> = Vec::with_capacity(32);
    let mut push = |rect: geometry::Rect, flags: u32, action: Action| {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        regions.push(InputRegionV2 {
            id: u64::try_from(actions.len()).unwrap_or(0) + 1,
            x: rect.left,
            y: rect.top,
            width: rect.width(),
            height: rect.height(),
            flags,
            ..Default::default()
        });
        actions.push(action);
    };
    let editable = state::presets_enabled(state.phase);
    if editable {
        for rect in state::dial_strips(&layout) {
            push(rect, INPUT_POINTER | INPUT_SCROLL, Action::Dial);
        }
        let now = Instant::now();
        let text = state::time_text(state.phase, state.seconds, now);
        let mut measure = |text: &str, size: f32| time_width(&target.host, text, size);
        let size = state::time_size(&layout, &text, &mut measure);
        let anchor = state::time_center(&layout, size);
        for (field, rect) in state::TimeField::ALL.into_iter().zip(state::field_rects(
            &text,
            size,
            anchor,
            &mut measure,
        )) {
            push(rect, INPUT_POINTER | INPUT_SCROLL, Action::Field(field));
        }
        for (index, rect) in layout.chips.iter().enumerate() {
            push(*rect, INPUT_POINTER, Action::Preset(index));
        }
    }
    if !matches!(state.phase, Phase::Idle) {
        push(layout.cancel, INPUT_POINTER, Action::Cancel);
    }
    push(layout.primary, INPUT_POINTER, Action::Primary);
    target
        .host
        .input()?
        .set_regions(target.page.id(), &regions)?;
    state.regions = actions;
    Ok(())
}

unsafe extern "C" fn create(
    info: *const PluginCreateInfoV2,
    out: *mut PluginHandleV2,
) -> PluginStatus {
    if info.is_null() || out.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host supplies a live create descriptor and writable handle output.
    let info = unsafe { &*info };
    // SAFETY: The host keeps its instance table allocated through shutdown and destruction.
    let host = match unsafe { Host::from_raw(info.host_api, info.plugin_token) } {
        Ok(host) => host,
        Err(_) => return PluginStatus::Internal,
    };
    match Timer::new(host.clone()) {
        Ok(timer) => {
            // SAFETY: The checked output receives an owned instance until destroy is called.
            unsafe {
                *out = Box::into_raw(Box::new(timer)).cast();
            }
            PluginStatus::Ok
        }
        Err(error) => {
            host.log()
                .write(3, &format!("Cannot create built-in timer: {error}"));
            PluginStatus::Internal
        }
    }
}

unsafe extern "C" fn shutdown(handle: *mut c_void) -> PluginStatus {
    if handle.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host has joined its worker and owns this live timer instance.
    let timer = unsafe { &mut *handle.cast::<Timer>() };
    timer.callbacks.clear();
    timer.translations.clear();
    timer.target.take();
    PluginStatus::Ok
}

unsafe extern "C" fn destroy(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: Successful shutdown stopped callbacks; the host destroys the instance once.
        drop(unsafe { Box::from_raw(handle.cast::<Timer>()) });
    }
}

unsafe extern "C" fn tick(handle: *mut c_void, widget: WidgetId, _: f64) -> PluginStatus {
    if handle.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host worker holds the live instance until it is joined before shutdown.
    let timer = unsafe { &*handle.cast::<Timer>() };
    let Some(target) = &timer.target else {
        return PluginStatus::StaleHandle;
    };
    // Drawing failures stay local: a non-Ok tick stops the worker and disables the plugin.
    if let Err(error) = poll(target, &timer.state) {
        target
            .host
            .log()
            .write(3, &format!("Built-in timer poll failed: {error}"));
    }
    if let Err(error) = timer.draw(widget) {
        target
            .host
            .log()
            .write(3, &format!("Built-in timer tick failed: {error}"));
    }
    PluginStatus::Ok
}
