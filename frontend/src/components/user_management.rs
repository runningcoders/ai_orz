use dioxus::prelude::*;
use tracing::info;
use crate::api::organization::{list_users_by_current_organization, create_user, UserListItem, CreateUserRequest};

#[component]
pub fn UserManagement() -> Element {
    let mut loading = use_signal(|| true);
    let mut creating = use_signal(|| false);
    let mut error = use_signal(|| String::new());
    let mut success = use_signal(|| String::new());
    let mut user_list = use_signal(|| Vec::<UserListItem>::new());

    // 创建用户对话框显示状态
    let mut show_create_dialog = use_signal(|| false);

    // 新建用户表单
    let mut new_username = use_signal(|| String::new());
    let mut new_display_name = use_signal(|| String::new());
    let mut new_email = use_signal(|| String::new());
    let mut new_password_hash = use_signal(|| String::new());
    let mut new_role = use_signal(|| 1); // 1 = Member, 2 = Admin, 3 = SuperAdmin

    // 加载用户列表
    let load_users = move || {
        spawn(async move {
            loading.set(true);
            error.set(String::new());
            match list_users_by_current_organization().await {
                Ok(users) => {
                    user_list.set(users);
                }
                Err(e) => {
                    error.set(format!("加载用户列表失败: {}", e));
                }
            }
            loading.set(false);
        });
    };

    // 页面加载时获取用户列表
    use_effect(move || {
        load_users();
    });

    // 打开创建用户对话框
    let open_create_dialog = move |_| {
        new_username.set(String::new());
        new_display_name.set(String::new());
        new_email.set(String::new());
        new_password_hash.set(String::new());
        new_role.set(1);
        error.set(String::new());
        success.set(String::new());
        show_create_dialog.set(true);
    };

    // 提交创建用户
    let on_create_submit = move |_| {
        spawn(async move {
            if new_username().is_empty() || new_password_hash().is_empty() {
                error.set("用户名和密码不能为空".to_string());
                return;
            }

            creating.set(true);
            error.set(String::new());
            success.set(String::new());

            let req = CreateUserRequest {
                username: new_username(),
                display_name: if new_display_name().is_empty() { None } else { Some(new_display_name()) },
                email: if new_email().is_empty() { None } else { Some(new_email()) },
                password_hash: new_password_hash(),
                role: new_role(),
            };

            match create_user(req).await {
                Ok(_) => {
                    success.set("用户创建成功！".to_string());
                    show_create_dialog.set(false);
                    load_users();
                }
                Err(e) => {
                    error.set(format!("创建用户失败: {}", e));
                }
            }
            creating.set(false);
        });
    };

    // 角色名称转换
    fn role_name(role: i32) -> &'static str {
        match role {
            1 => "成员",
            2 => "管理员",
            3 => "超级管理员",
            _ => "未知",
        }
    }

    // 状态名称转换
    fn status_name(status: i32) -> &'static str {
        match status {
            1 => "启用",
            _ => "禁用",
        }
    }

    rsx! {
        div {
            style: "
                max-width: 1000px;
                margin: 0 auto;
                background: white;
                border-radius: 12px;
                padding: 2rem;
                box-shadow: 0 2px 12px rgba(0,0,0,0.08);
            ",

            div {
                style: "
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    margin-bottom: 2rem;
                ",
                h2 {
                    style: "
                        color: #2c3e50;
                        font-size: 1.75rem;
                        margin: 0;
                    ",
                    "👥 用户管理"
                }
                button {
                    style: "
                        background: #2ecc71;
                        color: white;
                        border: none;
                        padding: 0.75rem 1.5rem;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 1rem;
                        font-weight: 500;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #27ae60;
                        }}
                    ",
                    onclick: open_create_dialog,
                    "+ 添加用户"
                }
            }

            // 错误消息
            if !error().is_empty() {
                div {
                    style: "
                        background: #fee;
                        border: 1px solid #faa;
                        border-radius: 6px;
                        padding: 1rem;
                        margin-bottom: 1.5rem;
                        color: #c33;
                    ",
                    "{error}"
                }
            }

            // 成功消息
            if !success().is_empty() {
                div {
                    style: "
                        background: #e8f5e9;
                        border: 1px solid #a5d6a7;
                        border-radius: 6px;
                        padding: 1rem;
                        margin-bottom: 1.5rem;
                        color: #2e7d32;
                    ",
                    "{success}"
                }
            }

            // 加载中
            if loading() {
                div {
                    style: "
                        text-align: center;
                        color: #666;
                        padding: 3rem;
                    ",
                    "正在加载用户列表..."
                }
            } else if user_list().is_empty() {
                div {
                    style: "
                        text-align: center;
                        color: #666;
                        padding: 3rem;
                    ",
                    "当前组织还没有用户，点击上方「添加用户」创建第一个用户吧！"
                }
            } else {
                // 用户列表表格
                div {
                    style: "
                        overflow-x: auto;
                    ",
                    table {
                        style: "
                            width: 100%;
                            border-collapse: collapse;
                            font-size: 0.95rem;
                        ",
                        thead {
                            tr {
                                style: "
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                ",
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: left;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "用户名"
                                }
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: left;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "显示名称"
                                }
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: left;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "邮箱"
                                }
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: left;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "角色"
                                }
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: left;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "状态"
                                }
                                th {
                                    style: "
                                        padding: 0.75rem;
                                        text-align: center;
                                        color: #495057;
                                        font-weight: 600;
                                    ",
                                    "操作"
                                }
                            }
                        }
                        tbody {
                            for user in user_list() {
                                tr {
                                    style: "
                                        border-bottom: 1px solid #dee2e6;
                                        &:hover {{
                                            background-color: #f8f9fa;
                                        }}
                                    ",
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                        ",
                                        "{user.username}"
                                    }
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                        ",
                                        "{user.display_name.clone().unwrap_or_default()}"
                                    }
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                        ",
                                        "{user.email.clone().unwrap_or_default()}"
                                    }
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                        ",
                                        {
                                            let style_str = match user.role {
                                                3 => "background: #e3f2fd; color: #1976d2; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem;",
                                                2 => "background: #e8f5e9; color: #2e7d32; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem;",
                                                _ => "background: #f5f5f5; color: #666; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem;",
                                            };
                                            rsx! {
                                                span {
                                                    style: "{style_str}",
                                                    "{role_name(user.role)}"
                                                }
                                            }
                                        }
                                    }
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                        ",
                                        {
                                            let style_str = if user.status == 1 {
                                                "background: #e8f5e9; color: #2e7d32; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem;"
                                            } else {
                                                "background: #ffebee; color: #c62828; padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.85rem;"
                                            };
                                            rsx! {
                                                span {
                                                    style: "{style_str}",
                                                    "{status_name(user.status)}"
                                                }
                                            }
                                        }
                                    }
                                    td {
                                        style: "
                                            padding: 0.75rem;
                                            text-align: center;
                                        ",
                                        // TODO: 编辑和删除功能
                                        button {
                                            style: "
                                                background: #e74c3c;
                                                color: white;
                                                border: none;
                                                padding: 0.4rem 0.8rem;
                                                border-radius: 4px;
                                                cursor: pointer;
                                                font-size: 0.85rem;
                                            ",
                                            onclick: move |_| {
                                                // TODO: 删除用户
                                                info!("Delete user: {}", user.user_id);
                                            },
                                            "删除"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 创建用户模态对话框
            if show_create_dialog() {
                // 遮罩
                div {
                    style: "
                        position: fixed;
                        top: 0;
                        left: 0;
                        right: 0;
                        bottom: 0;
                        background: rgba(0,0,0,0.5);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        z-index: 1000;
                    ",
                    onclick: move |_| {
                        show_create_dialog.set(false);
                    },

                    // 对话框内容
                    div {
                        style: "
                            background: white;
                            border-radius: 12px;
                            padding: 2rem;
                            width: 500px;
                            max-width: 90vw;
                            position: relative;
                        ",
                        onclick: |e| {
                            // 阻止点击内容区域关闭对话框
                            e.stop_propagation();
                        },

                        h3 {
                            style: "
                                color: #2c3e50;
                                margin-bottom: 1.5rem;
                                font-size: 1.5rem;
                            ",
                            "添加新用户"
                        }

                        form {
                            onsubmit: on_create_submit,

                            // 用户名
                            div {
                                style: "margin-bottom: 1.25rem;",
                                label {
                                    style: "
                                        display: block;
                                        color: #2c3e50;
                                        font-weight: 600;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "用户名 *"
                                }
                                input {
                                    r#type: "text",
                                    value: "{new_username}",
                                    oninput: move |e| new_username.set(e.value()),
                                    style: "
                                        width: 100%;
                                        padding: 0.75rem;
                                        border: 1px solid #ddd;
                                        border-radius: 6px;
                                        font-size: 1rem;
                                        box-sizing: border-box;
                                        &:focus {{
                                            outline: none;
                                            border-color: #3498db;
                                            box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                        }}
                                    ",
                                    placeholder: "请输入用户名",
                                    required: true,
                                }
                            }

                            // 显示名称
                            div {
                                style: "margin-bottom: 1.25rem;",
                                label {
                                    style: "
                                        display: block;
                                        color: #2c3e50;
                                        font-weight: 600;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "显示名称"
                                }
                                input {
                                    r#type: "text",
                                    value: "{new_display_name}",
                                    oninput: move |e| new_display_name.set(e.value()),
                                    style: "
                                        width: 100%;
                                        padding: 0.75rem;
                                        border: 1px solid #ddd;
                                        border-radius: 6px;
                                        font-size: 1rem;
                                        box-sizing: border-box;
                                        &:focus {{
                                            outline: none;
                                            border-color: #3498db;
                                            box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                        }}
                                    ",
                                    placeholder: "请输入显示名称（可选）",
                                }
                            }

                            // 邮箱
                            div {
                                style: "margin-bottom: 1.25rem;",
                                label {
                                    style: "
                                        display: block;
                                        color: #2c3e50;
                                        font-weight: 600;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "邮箱"
                                }
                                input {
                                    r#type: "email",
                                    value: "{new_email}",
                                    oninput: move |e| new_email.set(e.value()),
                                    style: "
                                        width: 100%;
                                        padding: 0.75rem;
                                        border: 1px solid #ddd;
                                        border-radius: 6px;
                                        font-size: 1rem;
                                        box-sizing: border-box;
                                        &:focus {{
                                            outline: none;
                                            border-color: #3498db;
                                            box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                        }}
                                    ",
                                    placeholder: "请输入邮箱（可选）",
                                }
                            }

                            // 密码哈希
                            div {
                                style: "margin-bottom: 1.25rem;",
                                label {
                                    style: "
                                        display: block;
                                        color: #2c3e50;
                                        font-weight: 600;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "密码哈希 *"
                                }
                                input {
                                    r#type: "password",
                                    value: "{new_password_hash}",
                                    oninput: move |e| new_password_hash.set(e.value()),
                                    style: "
                                        width: 100%;
                                        padding: 0.75rem;
                                        border: 1px solid #ddd;
                                        border-radius: 6px;
                                        font-size: 1rem;
                                        box-sizing: border-box;
                                        &:focus {{
                                            outline: none;
                                            border-color: #3498db;
                                            box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                        }}
                                    ",
                                    placeholder: "bcrypt 哈希后的密码",
                                    required: true,
                                }
                                p {
                                    style: "
                                        color: #999;
                                        font-size: 0.85rem;
                                        margin: 0.5rem 0 0 0;
                                    ",
                                    "前端需要提交 bcrypt 哈希，不要提交明文密码"
                                }
                            }

                            // 用户角色
                            div {
                                style: "margin-bottom: 2rem;",
                                label {
                                    style: "
                                        display: block;
                                        color: #2c3e50;
                                        font-weight: 600;
                                        margin-bottom: 0.5rem;
                                    ",
                                    "用户角色"
                                }
                                select {
                                    style: "
                                        width: 100%;
                                        padding: 0.75rem;
                                        border: 1px solid #ddd;
                                        border-radius: 6px;
                                        font-size: 1rem;
                                        background: white;
                                        &:focus {{
                                            outline: none;
                                            border-color: #3498db;
                                            box-shadow: 0 0 0 3px rgba(52, 152, 219, 0.2);
                                        }}
                                    ",
                                    onchange: move |e| {
                                        if let Ok(v) = e.value().parse() {
                                            new_role.set(v);
                                        }
                                    },
                                    option {
                                        value: "1",
                                        selected: new_role() == 1,
                                        "成员"
                                    }
                                    option {
                                        value: "2",
                                        selected: new_role() == 2,
                                        "管理员"
                                    }
                                    option {
                                        value: "3",
                                        selected: new_role() == 3,
                                        "超级管理员"
                                    }
                                }
                            }

                            // 按钮
                            div {
                                style: "
                                    display: flex;
                                    gap: 1rem;
                                    justify-content: flex-end;
                                ",
                                button {
                                    r#type: "button",
                                    style: "
                                        background: #95a5a6;
                                        color: white;
                                        border: none;
                                        padding: 0.75rem 1.5rem;
                                        border-radius: 6px;
                                        cursor: pointer;
                                        font-size: 1rem;
                                    ",
                                    onclick: move |_| {
                                        show_create_dialog.set(false);
                                    },
                                    "取消"
                                }
                                button {
                                    r#type: "submit",
                                    disabled: creating(),
                                    style: "
                                        background: #3498db;
                                        color: white;
                                        border: none;
                                        padding: 0.75rem 1.5rem;
                                        border-radius: 6px;
                                        cursor: pointer;
                                        font-size: 1rem;
                                        font-weight: 500;
                                        &:disabled {{
                                            background: #ccc;
                                            cursor: not-allowed;
                                        }}
                                    ",
                                    if creating() {
                                        "创建中..."
                                    } else {
                                        "创建用户"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
