const ALARM_SVG: &str = include_str!("../../../../resources/in_app/icons/alarm.fill.svg");

pub(crate) struct Text {
    pub(crate) title: &'static str,
    pub(crate) start: &'static str,
    pub(crate) pause: &'static str,
    pub(crate) resume: &'static str,
    pub(crate) cancel: &'static str,
    pub(crate) minutes: &'static str,
    pub(crate) ends_at: &'static str,
    pub(crate) done: &'static str,
    pub(crate) done_toast: &'static str,
    pub(crate) scroll_hint: &'static str,
    pub(crate) fine_hint: &'static str,
}

const EN: Text = Text {
    title: "Timer",
    start: "Start",
    pause: "Pause",
    resume: "Resume",
    cancel: "Cancel",
    minutes: "{} min",
    ends_at: "Ends {}",
    done: "Time's up",
    done_toast: "{} timer finished",
    scroll_hint: "Scroll to adjust",
    fine_hint: "Scroll to fine-tune",
};

const ZH_CN: Text = Text {
    title: "计时器",
    start: "开始",
    pause: "暂停",
    resume: "继续",
    cancel: "取消",
    minutes: "{} 分钟",
    ends_at: "{} 结束",
    done: "时间到",
    done_toast: "{} 计时已结束",
    scroll_hint: "滚动以调整",
    fine_hint: "滚动以微调",
};

pub(crate) const LANGUAGES: [&str; 2] = ["en_us", "zh_cn"];

pub(crate) fn text(language: &str) -> &'static Text {
    match language {
        "zh_cn" => &ZH_CN,
        _ => &EN,
    }
}

pub(crate) fn pairs(language: &str) -> [(&'static str, &'static str); 11] {
    let text = text(language);
    [
        ("timer_title", text.title),
        ("timer_start", text.start),
        ("timer_pause", text.pause),
        ("timer_resume", text.resume),
        ("timer_cancel", text.cancel),
        ("timer_minutes", text.minutes),
        ("timer_ends_at", text.ends_at),
        ("timer_done", text.done),
        ("timer_done_toast", text.done_toast),
        ("timer_scroll_hint", text.scroll_hint),
        ("timer_fine_hint", text.fine_hint),
    ]
}

pub(crate) fn fill(template: &str, value: &str) -> String {
    template.replacen("{}", value, 1)
}

/// The alarm glyph as SVG path data, taken from the icon the host already ships.
pub(crate) fn alarm_icon_path() -> &'static str {
    let Some(start) = ALARM_SVG.find("d=\"").map(|index| index + 3) else {
        return "";
    };
    let Some(length) = ALARM_SVG[start..].find('"') else {
        return "";
    };
    &ALARM_SVG[start..start + length]
}
