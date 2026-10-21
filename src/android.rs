//! Android entry points and window insets.
use std::sync::OnceLock;

use vse_ui::iced::{self, Padding};

static APP: OnceLock<iced::android::AndroidApp> = OnceLock::new();

#[unsafe(no_mangle)]
fn android_main(app: iced::android::AndroidApp) {
    // Android discards stderr, so route logs and panics to logcat.
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("pagecut"),
    );
    std::panic::set_hook(Box::new(|info| log::error!("{info}")));
    // `ndk_context` only holds the Application, which cannot start pickers.
    loki_file_access::set_activity(app.activity_as_ptr());
    let _ = APP.set(app.clone());
    iced::android::set_app(app);
    if let Err(error) = crate::run() {
        log::error!("{error}");
    }
}

/// Delivers document picker results from `android/MainActivity.kt`.
#[unsafe(no_mangle)]
extern "system" fn Java_dev_dioxus_main_MainActivity_onPickResult(
    mut env: jni::JNIEnv<'_>,
    _activity: jni::objects::JObject<'_>,
    uri: jni::objects::JString<'_>,
) {
    let uri = (!uri.is_null())
        .then(|| env.get_string(&uri).ok().map(String::from))
        .flatten();
    loki_file_access::on_activity_result(uri);
}

/// Space taken by the status and navigation bars, which `NativeActivity`
/// draws underneath.
pub fn system_bars() -> Padding {
    let Some(app) = APP.get() else {
        return Padding::ZERO;
    };
    let Some(window) = app.native_window() else {
        return Padding::ZERO;
    };
    let content = app.content_rect();
    // winit derives its scale factor from the density the same way.
    let scale = app.config().density().unwrap_or(160) as f32 / 160.0;
    let inset = |pixels: i32| pixels.max(0) as f32 / scale;
    Padding {
        top: inset(content.top),
        right: inset(window.width() - content.right),
        bottom: inset(window.height() - content.bottom),
        left: inset(content.left),
    }
}
