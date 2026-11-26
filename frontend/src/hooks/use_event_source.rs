//! 浏览器 `EventSource`（SSE）生命周期基建
//!
//! 统一「创建 → 挂 onmessage/onopen/onerror → 换 URL 重连 → 卸载关闭」的流程。
//! 业务侧只关心消息体解析；避免每处 SSE 各存一份 Resource + Closure + use_drop。
//!
//! ⚠️ `url` 是唯一的重连键：同一个 URL 下不会因为组件重渲染而重建连接。
//!    因此 `on_message` 等回调应只捕获 `Copy` 的 `Signal` / `Memo`（本项目的
//!    用法均如此），不要捕获每帧都会变化的值。

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// `EventSource` 的可选观测点
#[derive(Clone, Copy, Default)]
pub struct EventSourceOptions {
    /// 连接建立 / 重连成功
    pub on_open: Option<EventHandler<()>>,
    /// 连接出错（浏览器会按自身策略自动重连）
    pub on_error: Option<EventHandler<()>>,
    /// `EventSource::new` 直接失败（浏览器不支持 / URL 非法），不会自动重连
    pub on_create_error: Option<EventHandler<()>>,
}

struct EventSourceResource {
    event_source: web_sys::EventSource,
    /// 以下闭包在连接存续期间被 JS 持有，不能提前释放
    _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _on_open: Option<Closure<dyn FnMut(web_sys::Event)>>,
    _on_error: Option<Closure<dyn FnMut(web_sys::Event)>>,
}

impl Drop for EventSourceResource {
    fn drop(&mut self) {
        self.event_source.set_onmessage(None);
        self.event_source.set_onopen(None);
        self.event_source.set_onerror(None);
        self.event_source.close();
    }
}

/// 挂载一个 SSE 订阅
///
/// - `url` 为空：保持关闭（用于「未选中目标」的场景）
/// - `url` 变化：先关闭旧连接，再按新地址重建
/// - 组件卸载：`use_drop` 关闭连接并释放闭包
pub fn use_event_source(
    url: String,
    on_message: EventHandler<String>,
    options: EventSourceOptions,
) {
    let mut resource = use_signal(|| Option::<EventSourceResource>::None);

    use_effect(use_reactive!(|url| {
        // 换目标时先断旧连接：Drop 会解绑回调并 close
        resource.set(None);
        if url.is_empty() {
            return;
        }

        let on_message = on_message;
        let on_message_closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            on_message.call(data);
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);

        let event_source = match web_sys::EventSource::new(&url) {
            Ok(es) => es,
            Err(_) => {
                if let Some(cb) = options.on_create_error {
                    cb.call(());
                }
                return;
            }
        };
        event_source.set_onmessage(Some(on_message_closure.as_ref().unchecked_ref()));

        let on_open_closure = options.on_open.map(|cb| {
            Closure::wrap(Box::new(move |_: web_sys::Event| {
                cb.call(());
            }) as Box<dyn FnMut(web_sys::Event)>)
        });
        if let Some(closure) = &on_open_closure {
            event_source.set_onopen(Some(closure.as_ref().unchecked_ref()));
        }

        let on_error_closure = options.on_error.map(|cb| {
            Closure::wrap(Box::new(move |_: web_sys::Event| {
                cb.call(());
            }) as Box<dyn FnMut(web_sys::Event)>)
        });
        if let Some(closure) = &on_error_closure {
            event_source.set_onerror(Some(closure.as_ref().unchecked_ref()));
        }

        resource.set(Some(EventSourceResource {
            event_source,
            _on_message: on_message_closure,
            _on_open: on_open_closure,
            _on_error: on_error_closure,
        }));
    }));

    use_drop(move || {
        // 取走并立即释放：Drop 里不再触发一次无意义的信号通知
        drop(resource.take());
    });
}
