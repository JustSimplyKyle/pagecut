//! Run CPU work away from Iced's event loop and await it without blocking.
use eyre::{Context, Result};

pub async fn run<T: Send + 'static>(
    name: &'static str,
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::task::spawn_blocking(work).await.wrap_err(name)?
    }
    #[cfg(target_arch = "wasm32")]
    {
        ensure_threads()?;
        wasm_thread::Builder::new()
            .name(name.to_owned())
            .spawn(work)
            .wrap_err_with(|| format!("Cannot spawn {name}"))?
            .join_async()
            .await
            .map_err(|_| eyre::eyre!("{name} worker panicked"))?
    }
}

#[cfg(target_arch = "wasm32")]
fn ensure_threads() -> Result<()> {
    use wasm_bindgen::JsCast;
    let isolated = js_sys::Reflect::get(&js_sys::global(), &"crossOriginIsolated".into())
        .map_err(|error| eyre::eyre!("Cannot check browser isolation: {error:?}"))?
        .as_bool()
        .unwrap_or(false);
    eyre::ensure!(
        isolated,
        "Browser threads need cross-origin isolation. Start with `just dev-web`; hosting must send COOP: same-origin and COEP: require-corp."
    );
    let memory = wasm_bindgen::memory().unchecked_into::<js_sys::WebAssembly::Memory>();
    eyre::ensure!(
        memory
            .buffer()
            .is_instance_of::<js_sys::SharedArrayBuffer>(),
        "Wasm memory is not shared. Build with the project's atomics flags and `-Zbuild-std=std,panic_abort`."
    );
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn work_runs_on_another_thread_and_returns_its_result() {
        let caller = std::thread::current().id();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let worker = runtime
            .block_on(super::run("test", || Ok(std::thread::current().id())))
            .unwrap();
        assert_ne!(caller, worker);
        let error = runtime
            .block_on(super::run::<()>("test", || eyre::bail!("work failed")))
            .unwrap_err();
        assert!(error.to_string().contains("work failed"));
    }
}
