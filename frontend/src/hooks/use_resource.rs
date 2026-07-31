use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ResourceState<T> {
    Loading,
    Ready(T),
    Failed(String),
}

#[allow(dead_code)]
pub fn use_resource<T, F, Fut>(fetcher: F) -> (Signal<ResourceState<T>>, impl FnMut())
where
    T: Clone + 'static,
    F: Fn() -> Fut + Clone + 'static,
    Fut: std::future::Future<Output = Result<T, String>> + 'static,
{
    let mut state = use_signal(|| ResourceState::<T>::Loading);

    let reload = {
        let fetcher = fetcher.clone();
        move || {
            state.set(ResourceState::Loading);
            let fetcher = fetcher.clone();
            spawn(async move {
                match fetcher().await {
                    Ok(data) => state.set(ResourceState::Ready(data)),
                    Err(e) => state.set(ResourceState::Failed(e)),
                }
            });
        }
    };

    let mut reload_once = reload.clone();
    use_effect(move || {
        reload_once();
    });

    (state, reload)
}
