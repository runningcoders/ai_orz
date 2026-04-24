pub mod agent;
pub mod artifact;
pub mod cortex;
pub mod event_queue;
pub mod message;
pub mod model_provider;
pub mod organization;
pub mod user;
pub mod memory;
pub mod task;
pub mod project;
pub mod skill;
pub mod tool;

pub fn init_all(){
    agent::init();
    artifact::init();
    cortex::init();
    event_queue::init_message();
    message::init();
    model_provider::init();
    organization::init();
    user::init();
    memory::init();
    task::init();
    project::init();
    skill::init();
    tool::init();
}