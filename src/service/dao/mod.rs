pub mod agent;
pub mod artifact;
pub mod cortex;
pub mod event_queue;
pub mod message;
pub mod message_channel;
pub mod model_provider;
pub mod organization;
pub mod user;
pub mod memory;
pub mod task;
pub mod project;
pub mod skill;
pub mod tool;
pub mod tool_call;

// 消息推送渠道 DAO（无状态，不需要 init）
pub mod lark;
pub mod wechat;
pub mod slack;
pub mod email;
pub mod webhook;

pub fn init_all(){
    agent::init();
    artifact::init();
    cortex::init();
    event_queue::init_message();
    message::init();
    message_channel::init();
    model_provider::init();
    organization::init();
    user::init();
    memory::init();
    task::init();
    project::init();
    skill::init();
    tool::init();
    tool_call::init();
    // 消息推送渠道 DAO
    lark::init();
    wechat::init();
    slack::init();
    email::init();
    webhook::init();
}