use dioxus::prelude::*;

#[component]
pub fn Reception() -> Element {
    rsx! {
        div {
            style: "
                background: white;
                border-radius: 12px;
                padding: 3rem 2rem;
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
                text-align: center;
                max-width: 600px;
                margin: 0 auto;
            ",

            div {
                style: "
                    font-size: 4rem;
                    margin-bottom: 1.5rem;
                ",
                "👋"
            }

            h2 {
                style: "
                    color: #2c3e50;
                    margin-bottom: 1rem;
                    font-size: 1.75rem;
                ",
                "欢迎来到 AI Orz"
            }

            p {
                style: "
                    color: #666;
                    font-size: 1.1rem;
                    line-height: 1.6;
                    margin-bottom: 2rem;
                ",
                "您好！我是这里的前台接待。AI Orz 是一个智能的 AI 代理执行框架，帮助您组织和管理各类 AI 智能体，让它们协同工作完成复杂任务。"
            }

            div {
                style: "
                    display: flex;
                    gap: 1rem;
                    justify-content: center;
                    flex-wrap: wrap;
                ",
                button {
                    style: "
                        background: #3498db;
                        color: white;
                        border: none;
                        padding: 0.8rem 1.5rem;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 1rem;
                        transition: background-color 0.2s;
                        &:hover {{
                            background-color: #2980b9;
                        }}
                    ",
                    "开始使用"
                }
                button {
                    style: "
                        background: transparent;
                        color: #3498db;
                        border: 1px solid #3498db;
                        padding: 0.8rem 1.5rem;
                        border-radius: 6px;
                        cursor: pointer;
                        font-size: 1rem;
                        transition: all 0.2s;
                        &:hover {{
                            background-color: rgba(52, 152, 219, 0.1);
                        }}
                    ",
                    "了解更多"
                }
            }

            div {
                style: "
                    margin-top: 3rem;
                    padding-top: 2rem;
                    border-top: 1px solid #eee;
                    text-align: left;
                ",

                h3 {
                    style: "color: #2c3e50; margin-bottom: 1rem; font-size: 1.2rem;",
                    "📋 您可以在这里："
                }

                ul {
                    style: "color: #555; line-height: 1.8;",
                    li { "🏢 管理您的 AI 代理团队" }
                    li { "📋 组织和分配任务" }
                    li { "🤝 让多个智能体协作完成复杂工作" }
                    li { "📊 监控执行进度和结果" }
                }
            }
        }
    }
}
