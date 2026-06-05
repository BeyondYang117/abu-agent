use serde_json::Value;

use crate::chat::types::{ChatAssistantSnapshot, ContextUsageSegment};
use crate::mcp::ChatToolDefinition;
use crate::settings::{chat_no_think_instruction, default_chat_system_prompt, ChatToolsConfig};
use crate::skills;

use super::types::{AgentPhase, AgentStepResult, AgentStreamPolicy};

pub struct PrepareStepInput<'a> {
    pub step_number: u8,
    pub previous_steps: &'a [AgentStepResult],
    pub runtime_messages: &'a [Value],
    pub tools: &'a [ChatToolDefinition],
    pub phase: AgentPhase,
}

pub struct PreparedStep {
    pub active_tools: Vec<ChatToolDefinition>,
    pub runtime_messages: Vec<Value>,
    pub phase: AgentPhase,
    pub stream_policy: AgentStreamPolicy,
}

pub fn prepare_agent_step(input: PrepareStepInput<'_>) -> PreparedStep {
    let active_tools = match input.phase {
        AgentPhase::ToolLoop => input.tools.to_vec(),
        AgentPhase::Synthesis | AgentPhase::Plain => Vec::new(),
    };
    let stream_policy = match input.phase {
        AgentPhase::ToolLoop => AgentStreamPolicy::PlanningNoDoneUntilNoTools,
        AgentPhase::Synthesis | AgentPhase::Plain => AgentStreamPolicy::SynthesisAlwaysDone,
    };
    PreparedStep {
        active_tools,
        runtime_messages: input.runtime_messages.to_vec(),
        phase: input.phase,
        stream_policy,
    }
}

pub fn chat_tools_capable(
    provider: &crate::settings::ModelProvider,
    chat_tools: &ChatToolsConfig,
) -> bool {
    provider.supports_tools
        && (chat_tools.enabled || crate::settings::chat_native_tools_enabled(chat_tools))
}

pub fn apply_active_skill_tool_filter(
    tools: &mut Vec<ChatToolDefinition>,
    skill: &skills::SkillRecord,
) {
    if skill.allowed_tools.is_empty() {
        return;
    }
    tools.retain(|tool| {
        tool.source == "skill"
            || is_native_skill_tool_name(&tool.name)
            || is_kivio_builtin_tool(tool)
            || skill
                .allowed_tools
                .iter()
                .any(|recommended| tool_matches_recommended_name(tool, recommended))
    });
}

pub fn apply_assistant_tool_preset(
    tools: &mut Vec<ChatToolDefinition>,
    assistant_snapshot: Option<&ChatAssistantSnapshot>,
) {
    let preset = assistant_snapshot
        .map(|assistant| assistant.tool_preset.trim())
        .filter(|preset| !preset.is_empty())
        .unwrap_or("inherit");
    match preset {
        "none" => tools.clear(),
        "skills" => tools.retain(|tool| tool.source == "skill"),
        "inherit" | "all" => {}
        _ => {}
    }
}

pub fn apply_skill_fallback_when_tools_unavailable(
    chat_tools: &mut ChatToolsConfig,
    active_skill_id: Option<&str>,
    tools_available: bool,
) {
    if !tools_available
        && active_skill_id
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false)
        && chat_tools.skill_fallback_mode == "progressive"
    {
        chat_tools.skill_fallback_mode = "skill_md_only".to_string();
    }
}

pub fn available_builtin_tool_names(tools: &[ChatToolDefinition]) -> Vec<String> {
    let mut names = tools
        .iter()
        .filter(|tool| is_kivio_builtin_tool(tool))
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

pub fn disabled_builtin_tool_feedback(function_name: &str) -> Option<String> {
    const BUILTIN_NAMES: &[&str] = &[
        "web_search",
        "web_fetch",
        "read_file",
        "write_file",
        "edit_file",
        "run_command",
        "run_python",
    ];
    if BUILTIN_NAMES.contains(&function_name) {
        Some(format!(
            "Kivio tool `{function_name}` is not enabled for this chat. Do not call it again; answer using the available context and enabled tools only."
        ))
    } else {
        None
    }
}

pub fn is_native_skill_tool_name(name: &str) -> bool {
    matches!(
        name,
        "skill_activate" | "skill_read_file" | "skill_run_script"
    )
}

pub fn is_kivio_builtin_tool(tool: &ChatToolDefinition) -> bool {
    tool.source == "native" && !is_native_skill_tool_name(&tool.name)
}

pub fn builtin_tool_bypasses_approval(tool: &ChatToolDefinition) -> bool {
    (tool.source == "skill" && is_native_skill_tool_name(&tool.name)) || tool.source == "native"
}

pub fn build_chat_system_prompt(
    language: &str,
    has_image: bool,
    thinking_enabled: bool,
    registry: &skills::SkillRegistry,
    chat_tools: &ChatToolsConfig,
    tools_available: bool,
    available_builtin_tools: &[String],
    active_skill_id: Option<&str>,
    active_skill_detail: Option<&skills::SkillDetail>,
    assistant_snapshot: Option<&ChatAssistantSnapshot>,
    custom_system_prompt: &str,
) -> String {
    build_chat_system_prompt_with_segments(
        language,
        has_image,
        thinking_enabled,
        registry,
        chat_tools,
        tools_available,
        available_builtin_tools,
        active_skill_id,
        active_skill_detail,
        assistant_snapshot,
        custom_system_prompt,
    )
    .0
}

pub fn build_chat_system_prompt_with_segments(
    language: &str,
    has_image: bool,
    thinking_enabled: bool,
    registry: &skills::SkillRegistry,
    chat_tools: &ChatToolsConfig,
    tools_available: bool,
    available_builtin_tools: &[String],
    active_skill_id: Option<&str>,
    active_skill_detail: Option<&skills::SkillDetail>,
    assistant_snapshot: Option<&ChatAssistantSnapshot>,
    custom_system_prompt: &str,
) -> (String, Vec<ContextUsageSegment>) {
    let mut prompt = String::new();
    let mut segments = Vec::new();
    let base_prompt = if custom_system_prompt.trim().is_empty() {
        default_chat_system_prompt(language, has_image)
    } else {
        custom_system_prompt.trim().to_string()
    };
    append_context_segment(
        &mut prompt,
        &mut segments,
        "system_prompt",
        "System prompt",
        &base_prompt,
    );
    if let Some(assistant) = assistant_snapshot {
        let assistant_prompt = assistant_prompt_segment(assistant);
        if !assistant_prompt.trim().is_empty() {
            append_context_segment(
                &mut prompt,
                &mut segments,
                "assistant",
                "Assistant",
                &assistant_prompt,
            );
        }
    }
    append_context_segment(
        &mut prompt,
        &mut segments,
        "runtime_context",
        "Runtime context",
        &crate::settings::chat_current_datetime_context(language),
    );

    if tools_available {
        let mut action_examples = vec!["activating a skill", "reading a file", "running a script"];
        if available_builtin_tools
            .iter()
            .any(|tool| matches!(tool.as_str(), "web_search" | "web_fetch"))
        {
            action_examples.push("using the web");
        }
        let mut runtime = format!(
            "You have access to tools (functions). When the user's request requires action—such as {}—YOU MUST call the appropriate enabled tool instead of describing what to do. Never say \"I cannot run commands\" or \"you can do it yourself\" when an enabled tool is available for that action. Do not call tools that are not listed as enabled.",
            action_examples.join(", ")
        );
        runtime.push_str(
            " Only claim that a tool was used, a script was run, a file was read, or the web was searched after Kivio returns an actual tool result in the conversation.",
        );
        if language.starts_with("zh") {
            runtime.push_str(
                " 若用户只问今天/明天/星期几等可由上文「当前本地时间」直接推算的日期问题，直接回答，不要调用工具。",
            );
        } else {
            runtime.push_str(
                " If the user only asks for today/tomorrow/weekday derivable from the system local time above, answer directly without calling tools.",
            );
        }
        append_context_segment(
            &mut prompt,
            &mut segments,
            "runtime_context",
            "Runtime context",
            &runtime,
        );
        if let Some(native_prompt) = native_tools_prompt(available_builtin_tools, language) {
            append_context_segment(
                &mut prompt,
                &mut segments,
                "native_tools",
                "Native tools",
                &native_prompt,
            );
        }
    }

    let include_catalog = chat_tools.skill_auto_match
        || active_skill_id.is_some()
        || chat_tools.skill_fallback_mode != "legacy_full_body";
    if include_catalog {
        let catalog =
            skills::format_catalog(registry, active_skill_id, tools_available, |skill_id| {
                crate::settings::is_skill_enabled(chat_tools, skill_id)
            });
        if !catalog.is_empty() {
            append_context_segment(&mut prompt, &mut segments, "skills", "Skills", &catalog);
        }
    }

    if !chat_tools.skill_auto_match {
        append_context_segment(
            &mut prompt,
            &mut segments,
            "skills",
            "Skills",
            "Only activate skills that are enabled in Settings (listed in the catalog below).",
        );
    }

    let fallback = chat_tools.skill_fallback_mode.as_str();
    if let Some(skill_id) = active_skill_id.filter(|id| !id.trim().is_empty()) {
        let mut skill_prompt = format!("User pinned skill for this message: {skill_id}");
        if tools_available {
            skill_prompt.push_str(
                ". Call skill_activate with this name only because the user pinned it; otherwise prefer Kivio built-in tools when they fit.",
            );
        } else if matches!(fallback, "skill_md_only" | "legacy_full_body") {
            skill_prompt.push_str(". Follow the Active Skill instructions below.");
        } else {
            skill_prompt.push_str(
                ". Progressive skill loading requires tool support; switch provider or set fallback to SKILL.md only.",
            );
        }
        append_context_segment(
            &mut prompt,
            &mut segments,
            "skills",
            "Skills",
            &skill_prompt,
        );
    } else if tools_available && chat_tools.skill_auto_match {
        let builtin_hint = if available_builtin_tools.is_empty() {
            "Kivio built-in tools".to_string()
        } else {
            format!("Kivio 内置工具（{}）", available_builtin_tools.join(", "))
        };
        if language.starts_with("zh") {
            append_context_segment(
                &mut prompt,
                &mut segments,
                "skills",
                "Skills",
                &format!("Skill 目录仅供参考：仅当用户明确需要某个 Skill 的能力（或点名 Skill 名称）时才 skill_activate。泛泛请求若已启用 {builtin_hint} 能覆盖，应优先使用对应内置工具；不要只因 Skill 描述里提到 Python/脚本/联网就激活无关 Skill。"),
            );
        } else {
            let builtin_hint = if available_builtin_tools.is_empty() {
                "Kivio built-in tools".to_string()
            } else {
                format!(
                    "Kivio built-in tools ({})",
                    available_builtin_tools.join(", ")
                )
            };
            append_context_segment(
                &mut prompt,
                &mut segments,
                "skills",
                "Skills",
                &format!("The skill catalog is optional: call skill_activate only when the user clearly needs that skill (or names it). For generic requests covered by enabled {builtin_hint}, prefer the corresponding built-in tool instead of activating an unrelated skill just because its description mentions Python, scripts, or web access."),
            );
        }
    }

    if matches!(fallback, "skill_md_only" | "legacy_full_body") {
        if let Some(skill) = active_skill_detail {
            if !skill.body.trim().is_empty() {
                append_context_segment(
                    &mut prompt,
                    &mut segments,
                    "skills",
                    "Skills",
                    &format!("Active Skill:\n{}", skill.body),
                );
            }
        }
    }

    if !thinking_enabled && !tools_available {
        append_context_segment(
            &mut prompt,
            &mut segments,
            "runtime_context",
            "Runtime context",
            chat_no_think_instruction(language),
        );
    }
    (prompt, merge_context_segments(segments))
}

fn append_context_segment(
    prompt: &mut String,
    segments: &mut Vec<ContextUsageSegment>,
    id: &str,
    label: &str,
    content: &str,
) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str(trimmed);
    segments.push(ContextUsageSegment {
        id: id.to_string(),
        label: label.to_string(),
        estimated_tokens: estimate_tokens(trimmed),
        color: context_segment_color(id).map(str::to_string),
    });
}

fn assistant_prompt_segment(assistant: &ChatAssistantSnapshot) -> String {
    let mut parts = vec![format!("Active assistant: {}", assistant.name)];
    if !assistant.description.trim().is_empty() {
        parts.push(format!(
            "Assistant purpose: {}",
            assistant.description.trim()
        ));
    }
    if !assistant.system_prompt.trim().is_empty() {
        parts.push(format!(
            "Assistant instructions:\n{}",
            assistant.system_prompt.trim()
        ));
    }
    if !assistant.greeting.trim().is_empty() {
        parts.push(format!("Assistant greeting: {}", assistant.greeting.trim()));
    }
    if !assistant.conversation_starters.is_empty() {
        parts.push(format!(
            "Representative starter prompts: {}",
            assistant.conversation_starters.join(" | ")
        ));
    }
    parts.join("\n\n")
}

pub fn merge_context_segments(segments: Vec<ContextUsageSegment>) -> Vec<ContextUsageSegment> {
    let mut merged: Vec<ContextUsageSegment> = Vec::new();
    for segment in segments {
        if segment.estimated_tokens == 0 {
            continue;
        }
        if let Some(existing) = merged.iter_mut().find(|item| item.id == segment.id) {
            existing.estimated_tokens += segment.estimated_tokens;
        } else {
            merged.push(segment);
        }
    }
    merged
}

pub fn context_segment_color(id: &str) -> Option<&'static str> {
    match id {
        "system_prompt" => Some("#7A7A7A"),
        "assistant" => Some("#8A6FBD"),
        "runtime_context" => Some("#3E8B60"),
        "tool_definitions" => Some("#7553CF"),
        "skills" => Some("#BD8A3E"),
        "mcp" => Some("#B04B8D"),
        "native_tools" => Some("#4E7FB8"),
        "summarized_conversation" => Some("#BF3F66"),
        "conversation" => Some("#D07652"),
        "attachments" => Some("#6A8FBD"),
        _ => None,
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut non_ascii = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            non_ascii += 1;
        }
    }
    ascii.div_ceil(4) + non_ascii
}

fn tool_matches_recommended_name(tool: &ChatToolDefinition, recommended: &str) -> bool {
    let recommended = recommended.trim();
    if recommended.is_empty() {
        return false;
    }
    tool.name == recommended
        || tool.id == recommended
        || tool.openai_tool_name() == recommended
        || tool
            .server_id
            .as_deref()
            .map(|server_id| format!("{server_id}:{}", tool.name) == recommended)
            .unwrap_or(false)
}

fn native_tools_prompt(available_builtin_tools: &[String], language: &str) -> Option<String> {
    if available_builtin_tools.is_empty() {
        return None;
    }
    let list = available_builtin_tools.join(", ");
    let has_web_search = available_builtin_tools
        .iter()
        .any(|tool| tool.as_str() == "web_search");
    let has_web_fetch = available_builtin_tools
        .iter()
        .any(|tool| tool.as_str() == "web_fetch");
    let zh_live_access_hint = match (has_web_search, has_web_fetch) {
        (true, true) => "实时搜索或网页读取必须优先用 web_search/web_fetch 或对应 Skill 脚本。",
        (true, false) => "实时搜索必须优先用 web_search 或对应 Skill 脚本。",
        (false, true) => "网页读取必须优先用 web_fetch 或对应 Skill 脚本。",
        (false, false) => "需要联网/API 访问时，请启用对应联网工具或使用对应 Skill 脚本。",
    };
    let en_live_access_hint = match (has_web_search, has_web_fetch) {
        (true, true) => {
            "Use web_search/web_fetch or the relevant Skill script for live web/API access."
        }
        (true, false) => "Use web_search or the relevant Skill script for live web/API access.",
        (false, true) => "Use web_fetch or the relevant Skill script for web page access.",
        (false, false) => {
            "Enable the relevant web tool or use the relevant Skill script for live web/API access."
        }
    };
    let prompt = if language.starts_with("zh") {
        format!(
            "Kivio 内置工具（已启用）：{list}。只允许调用这里列出的内置工具。文件路径须在用户主目录内；可选工作区根目录进一步收紧。write_file、edit_file、run_command 会请求用户确认；run_command 非零退出码代表执行失败，不要用它运行 Skill 自带脚本，Skill 脚本必须走 skill_run_script。run_command 不得用 pip/pip3/python -m pip 安装包来绕过 run_python 沙盒失败；只有用户明确要求修改本机 Python 环境时，才能设置 allow_host_python_package_install=true 且使用 --user 或虚拟环境。run_python 在 Pyodide 沙盒中运行，无本机文件系统和联网能力；导入 numpy、matplotlib、pandas、scipy、sympy、scikit-learn、statsmodels、pillow、seaborn、micropip 等常用包时会自动加载，适合数据运算、统计分析、机器学习基础分析和生成图表；用 run_python 生成图像/图表时，保存为 Pyodide 当前目录下的相对文件名（例如 output.png），不要保存到 /Users 等本机路径，不要 print base64 或 data:image URL；Kivio 会自动捕获并渲染生成的图片。不要在 run_python 里使用 tavily、requests、httpx、urllib3、aiohttp 等联网/API 客户端，{zh_live_access_hint}不要为了这些 Python 包使用 host pip 安装，除非用户明确要求操作本机环境。用户要用 Python 跑代码/计算时优先 run_python，不要用 skill_run_script，除非用户点名某个 Skill。"
        )
    } else {
        format!(
            "Kivio built-in tools enabled: {list}. Only call built-in tools listed here. Paths must stay under the user home directory (optional workspace roots further restrict). write_file, edit_file, and run_command require user approval; run_command treats non-zero exit codes as failures. Do not use run_command to run Skill bundled scripts; use skill_run_script. Do not use pip/pip3/python -m pip through run_command to bypass run_python sandbox failures; only set allow_host_python_package_install=true when the user explicitly asks to modify the host Python environment, and then use --user or a virtual environment. run_python runs in a Pyodide sandbox with no host filesystem or network access and auto-loads common packages when imported, including numpy, matplotlib, pandas, scipy, sympy, scikit-learn, statsmodels, pillow, seaborn, and micropip; use it for data computation, statistical analysis, basic machine-learning analysis, code execution, and charts. When generating images/charts with run_python, save them to relative filenames in the Pyodide current directory such as output.png; do not save to host paths such as /Users, and do not print base64 or data:image URLs. Kivio captures and renders generated images automatically. Do not use network/API clients such as tavily, requests, httpx, urllib3, or aiohttp in run_python; {en_live_access_hint} Do not use host pip to install these Python packages unless the user explicitly asks to modify the host Python environment. For generic Python requests, use run_python—not skill_run_script—unless the user named a specific skill."
        )
    };
    Some(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_assistant_snapshot(tool_preset: &str, skill_id: Option<&str>) -> ChatAssistantSnapshot {
        ChatAssistantSnapshot {
            id: "asst_test".to_string(),
            name: "Test Assistant".to_string(),
            description: String::new(),
            system_prompt: String::new(),
            provider_id: String::new(),
            model: String::new(),
            skill_id: skill_id.map(str::to_string),
            tool_preset: tool_preset.to_string(),
            conversation_starters: Vec::new(),
            greeting: String::new(),
        }
    }

    fn test_mcp_tool() -> ChatToolDefinition {
        ChatToolDefinition {
            id: "mcp__demo__search".to_string(),
            name: "search".to_string(),
            description: "Search demo".to_string(),
            source: "mcp".to_string(),
            server_id: Some("demo".to_string()),
            server_name: Some("Demo".to_string()),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
            sensitive: false,
        }
    }

    #[test]
    fn is_native_skill_tool_name_matches_runtime_tools() {
        assert!(is_native_skill_tool_name("skill_activate"));
        assert!(is_native_skill_tool_name("skill_run_script"));
        assert!(!is_native_skill_tool_name("web_search"));
    }

    #[test]
    fn chat_prompt_omits_disabled_web_tools() {
        let registry = skills::SkillRegistry::default();
        let mut chat_tools = crate::settings::ChatToolsConfig::default();
        chat_tools.native_tools.skill_runtime = true;
        chat_tools.native_tools.run_python = true;
        chat_tools.native_tools.web_search = false;
        chat_tools.native_tools.web_fetch = false;

        let prompt = build_chat_system_prompt(
            "zh-CN",
            false,
            false,
            &registry,
            &chat_tools,
            true,
            &["run_python".to_string()],
            None,
            None,
            None,
            "",
        );

        assert!(prompt.contains("run_python"));
        assert!(!prompt.contains("web_search"));
        assert!(!prompt.contains("web_fetch"));
    }

    #[test]
    fn assistant_tool_preset_none_disables_all_tools() {
        let assistant = test_assistant_snapshot("none", Some("doc"));
        let mut tools = vec![
            crate::mcp::types::native_skill_activate_tool(),
            crate::mcp::types::native_web_fetch_tool(),
            test_mcp_tool(),
        ];

        apply_assistant_tool_preset(&mut tools, Some(&assistant));

        assert!(tools.is_empty());
    }

    #[test]
    fn assistant_tool_preset_skills_keeps_only_skill_runtime_tools() {
        let assistant = test_assistant_snapshot("skills", Some("doc"));
        let mut tools = vec![
            crate::mcp::types::native_skill_activate_tool(),
            crate::mcp::types::native_skill_read_file_tool(),
            crate::mcp::types::native_web_fetch_tool(),
            test_mcp_tool(),
        ];

        apply_assistant_tool_preset(&mut tools, Some(&assistant));

        assert_eq!(tools.len(), 2);
        assert!(tools.iter().all(|tool| tool.source == "skill"));
        assert!(tools.iter().any(|tool| tool.name == "skill_activate"));
        assert!(tools.iter().any(|tool| tool.name == "skill_read_file"));
    }

    #[test]
    fn assistant_tool_preset_inherit_and_all_leave_tools_unchanged() {
        for preset in ["inherit", "all", "unexpected"] {
            let assistant = test_assistant_snapshot(preset, None);
            let mut tools = vec![
                crate::mcp::types::native_skill_activate_tool(),
                crate::mcp::types::native_web_fetch_tool(),
                test_mcp_tool(),
            ];

            apply_assistant_tool_preset(&mut tools, Some(&assistant));

            assert_eq!(tools.len(), 3, "preset {preset} should not filter tools");
        }
    }

    #[test]
    fn skill_fallback_switches_to_markdown_when_assistant_disables_tools() {
        let mut chat_tools = crate::settings::ChatToolsConfig::default();

        apply_skill_fallback_when_tools_unavailable(&mut chat_tools, Some("doc"), false);

        assert_eq!(chat_tools.skill_fallback_mode, "skill_md_only");
    }

    #[test]
    fn disabled_builtin_tool_feedback_is_hidden_model_feedback() {
        let feedback = disabled_builtin_tool_feedback("web_search")
            .expect("disabled builtin tools should produce model feedback");

        assert!(feedback.contains("not enabled"));
        assert!(feedback.contains("web_search"));
        assert!(disabled_builtin_tool_feedback("mcp__server__tool").is_none());
    }

    #[test]
    fn estimate_tokens_counts_ascii_and_cjk() {
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("你好ab"), 3);
    }
}
