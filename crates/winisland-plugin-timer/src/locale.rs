const ALARM_SVG: &str = include_str!("../../../resources/in_app/icons/alarm.fill.svg");

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

const ZH_TW: Text = Text {
    title: "計時器",
    start: "開始",
    pause: "暫停",
    resume: "繼續",
    cancel: "取消",
    minutes: "{} 分鐘",
    ends_at: "{} 結束",
    done: "時間到",
    done_toast: "{} 計時已結束",
    scroll_hint: "滾動以調整",
    fine_hint: "滾動以微調",
};

const ES_ES: Text = Text {
    title: "Temporizador",
    start: "Iniciar",
    pause: "Pausar",
    resume: "Reanudar",
    cancel: "Cancelar",
    minutes: "{} min",
    ends_at: "Termina a las {}",
    done: "Tiempo agotado",
    done_toast: "Temporizador de {} terminado",
    scroll_hint: "Desplaza para ajustar",
    fine_hint: "Desplaza para ajuste fino",
};

const JA_JP: Text = Text {
    title: "タイマー",
    start: "開始",
    pause: "一時停止",
    resume: "再開",
    cancel: "キャンセル",
    minutes: "{} 分",
    ends_at: "{} に終了",
    done: "時間になりました",
    done_toast: "{} のタイマーが終了しました",
    scroll_hint: "スクロールして調整",
    fine_hint: "スクロールして微調整",
};

const KO_KR: Text = Text {
    title: "타이머",
    start: "시작",
    pause: "일시 정지",
    resume: "계속",
    cancel: "취소",
    minutes: "{}분",
    ends_at: "{}에 종료",
    done: "시간이 되었습니다",
    done_toast: "{} 타이머가 끝났습니다",
    scroll_hint: "스크롤하여 조정",
    fine_hint: "스크롤하여 미세 조정",
};

pub(crate) const LANGUAGES: [&str; 6] = ["en_us", "zh_cn", "zh_tw", "es_es", "ja_jp", "ko_kr"];

pub(crate) fn text(language: &str) -> &'static Text {
    match language {
        "zh_cn" => &ZH_CN,
        "zh_tw" => &ZH_TW,
        "es_es" => &ES_ES,
        "ja_jp" => &JA_JP,
        "ko_kr" => &KO_KR,
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
