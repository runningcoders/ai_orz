use dioxus::prelude::*;
use js_sys::Math;

#[derive(Clone, Debug)]
pub struct Employee {
    id: String,
    name: String,
    email: String,
    role: EmployeeRole,
    status: EmployeeStatus,
}

#[derive(Clone, Debug)]
pub enum EmployeeRole {
    Admin,
    User,
    Guest,
}

impl EmployeeRole {
    fn to_str(&self) -> &str {
        match self {
            EmployeeRole::Admin => "管理员",
            EmployeeRole::User => "普通用户",
            EmployeeRole::Guest => "访客",
        }
    }
}

#[derive(Clone, Debug)]
pub enum EmployeeStatus {
    Active,
    Inactive,
}

impl EmployeeStatus {
    fn to_str(&self) -> &str {
        match self {
            EmployeeStatus::Active => "在职",
            EmployeeStatus::Inactive => "离职",
        }
    }

    fn color(&self) -> &str {
        match self {
            EmployeeStatus::Active => "#2ecc71",
            EmployeeStatus::Inactive => "#95a5a6",
        }
    }
}

fn generate_id() -> String {
    let now = Math::random() * 1000000000.0;
    format!("{}{}", now as u64, (Math::random() * 10000.0) as u32)
}

#[component]
pub fn EmployeeManagement() -> Element {
    let mut employees = use_signal(Vec::<Employee>::new);
    let mut loading = use_signal(|| true);
    let mut show_add_modal = use_signal(|| false);
    let mut new_employee_name = use_signal(String::new);
    let mut new_employee_email = use_signal(String::new);

    // 模拟加载数据
    use_effect(move || {
        let sample_employees = vec![
            Employee {
                id: generate_id(),
                name: "张三".to_string(),
                email: "zhangsan@example.com".to_string(),
                role: EmployeeRole::Admin,
                status: EmployeeStatus::Active,
            },
            Employee {
                id: generate_id(),
                name: "李四".to_string(),
                email: "lisi@example.com".to_string(),
                role: EmployeeRole::User,
                status: EmployeeStatus::Active,
            },
            Employee {
                id: generate_id(),
                name: "王五".to_string(),
                email: "wangwu@example.com".to_string(),
                role: EmployeeRole::Guest,
                status: EmployeeStatus::Inactive,
            },
        ];

        employees.set(sample_employees);
        loading.set(false);
    });

    let employees_cloned = employees.read().clone();

    rsx! {
        div {
            style: "
                max-width: 900px;
                margin: 0 auto;
                background: white;
                border-radius: 12px;
                padding: 2rem;
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
            ",

            // 标题 + 添加按钮
            div {
                style: "
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    margin-bottom: 2rem;
                ",
                h2 {
                    style: "color: #2c3e50; margin: 0;",
                    "员工管理"
                }
                button {
                    style: "
                        background: #27ae60;
                        color: white;
                        border: none;
                        padding: 0.75rem 1.25rem;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 1rem;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #219a52;
                        }}
                    ",
                    onclick: move |_| show_add_modal.set(true),
                    "+ 添加员工"
                }
            }

            // 加载状态
            if *loading.read() {
                div {
                    style: "text-align: center; padding: 3rem; color: #666;",
                    "加载中..."
                }
            } else if employees_cloned.is_empty() {
                div {
                    style: "
                        text-align: center;
                        padding: 4rem 2rem;
                        color: #666;
                    ",
                    "暂无员工，点击上方按钮添加第一个员工"
                }
            } else {
                // 员工列表
                table {
                    style: "
                        width: 100%;
                        border-collapse: collapse;
                    ",
                    thead {
                        tr {
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "姓名"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "邮箱"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "角色"
                            }
                            th {
                                style: "
                                    text-align: left;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "状态"
                            }
                            th {
                                style: "
                                    text-align: center;
                                    padding: 0.75rem;
                                    background-color: #f8f9fa;
                                    border-bottom: 2px solid #dee2e6;
                                    color: #333;
                                ",
                                "操作"
                            }
                        }
                    }
                    tbody {
                        {
                            employees_cloned.iter().map(|employee| rsx! {
                                tr {
                                    key: "{employee.id}",
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            font-weight: 500;
                                            color: #2c3e50;
                                        ",
                                        "{employee.name}"
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            color: #666;
                                        ",
                                        "{employee.email}"
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                        ",
                                        "{employee.role.to_str()}"
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                        ",
                                        span {
                                            style: "
                                                display: inline-block;
                                                padding: 0.25rem 0.75rem;
                                                border-radius: 12px;
                                                color: white;
                                                font-size: 0.8rem;
                                                background-color: {employee.status.color()};
                                            ",
                                            "{employee.status.to_str()}"
                                        }
                                    }
                                    td {
                                        style: "
                                            padding: 1rem 0.75rem;
                                            border-bottom: 1px solid #dee2e6;
                                            text-align: center;
                                        ",
                                        div {
                                            style: "display: flex; gap: 0.5rem; justify-content: center;",
                                            button {
                                                style: "
                                                    background: #3498db;
                                                    color: white;
                                                    border: none;
                                                    padding: 0.4rem 0.8rem;
                                                    border-radius: 4px;
                                                    cursor: pointer;
                                                    font-size: 0.85rem;
                                                ",
                                                "编辑"
                                            }
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
                                                "删除"
                                            }
                                        }
                                    }
                                }
                            })
                        }
                    }
                }
            }

            // 添加员工弹窗
            if *show_add_modal.read() {
                div {
                    style: "
                        position: fixed;
                        top: 0;
                        left: 0;
                        right: 0;
                        bottom: 0;
                        background-color: rgba(0, 0, 0, 0.5);
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        z-index: 200;
                    ",
                    onclick: move |_| show_add_modal.set(false),
                    div {
                        style: "
                            background: white;
                            border-radius: 12px;
                            padding: 2rem;
                            width: 500px;
                            max-width: 90vw;
                        ",
                        onclick: |e| e.stop_propagation(),
                        h3 {
                            style: "margin-top: 0; margin-bottom: 1.5rem; color: #2c3e50;",
                            "添加新员工"
                        }
                        div {
                            style: "margin-bottom: 1.5rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "员工姓名"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_employee_name}",
                                oninput: move |e| new_employee_name.set(e.value()),
                                placeholder: "请输入员工姓名"
                            }
                        }
                        div {
                            style: "margin-bottom: 2rem;",
                            label {
                                style: "
                                    display: block;
                                    margin-bottom: 0.5rem;
                                    color: #333;
                                    font-weight: 500;
                                ",
                                "邮箱地址"
                            }
                            input {
                                style: "
                                    width: 100%;
                                    padding: 0.75rem;
                                    border: 1px solid #ddd;
                                    border-radius: 6px;
                                    font-size: 1rem;
                                ",
                                value: "{new_employee_email}",
                                oninput: move |e| new_employee_email.set(e.value()),
                                placeholder: "请输入邮箱地址"
                            }
                        }
                        div {
                            style: "
                                display: flex;
                                gap: 1rem;
                                justify-content: flex-end;
                            ",
                            button {
                                style: "
                                    background: #95a5a6;
                                    color: white;
                                    border: none;
                                    padding: 0.75rem 1.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                ",
                                onclick: move |_| {
                                    show_add_modal.set(false);
                                    new_employee_name.set(String::new());
                                    new_employee_email.set(String::new());
                                },
                                "取消"
                            }
                            button {
                                style: "
                                    background: #27ae60;
                                    color: white;
                                    border: none;
                                    padding: 0.75rem 1.5rem;
                                    border-radius: 6px;
                                    cursor: pointer;
                                ",
                                onclick: move |_| {
                                    // TODO: 调用后端 API 创建员工
                                    if !new_employee_name().is_empty() && !new_employee_email().is_empty() {
                                        let mut employees = employees.write();
                                        employees.push(Employee {
                                            id: generate_id(),
                                            name: new_employee_name(),
                                            email: new_employee_email(),
                                            role: EmployeeRole::User,
                                            status: EmployeeStatus::Active,
                                        });
                                        show_add_modal.set(false);
                                        new_employee_name.set(String::new());
                                        new_employee_email.set(String::new());
                                    }
                                },
                                "添加"
                            }
                        }
                    }
                }
            }
        }
    }
}
