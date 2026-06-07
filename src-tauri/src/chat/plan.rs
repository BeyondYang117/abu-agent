use crate::chat::types::{AgentPlanMode, AgentPlanState, AgentPlanStatus};

pub fn mode_from_str(value: &str) -> Result<AgentPlanMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "act" => Ok(AgentPlanMode::Act),
        "plan" => Ok(AgentPlanMode::Plan),
        other => Err(format!("Unknown agent plan mode: {other}")),
    }
}

pub fn is_plan_mode(state: &AgentPlanState) -> bool {
    state.mode == AgentPlanMode::Plan
}

pub fn with_mode(current: &AgentPlanState, mode: AgentPlanMode) -> AgentPlanState {
    let mut next = current.clone();
    if next.mode != mode {
        next.mode = mode;
        next.updated_at = chrono::Local::now().timestamp();
    }
    next
}

pub fn approve(current: &AgentPlanState) -> AgentPlanState {
    let mut next = current.clone();
    next.mode = AgentPlanMode::Act;
    next.status = if current_plan_text(current).is_some() {
        AgentPlanStatus::Approved
    } else {
        AgentPlanStatus::Empty
    };
    next.updated_at = chrono::Local::now().timestamp();
    next
}

pub fn capture_draft_from_reply(current: &AgentPlanState, content: &str) -> AgentPlanState {
    let plan = content.trim();
    if plan.is_empty() {
        return current.clone();
    }
    AgentPlanState {
        mode: AgentPlanMode::Plan,
        status: AgentPlanStatus::Draft,
        plan: Some(plan.to_string()),
        updated_at: chrono::Local::now().timestamp(),
    }
}

pub fn format_prompt(state: &AgentPlanState, language: &str) -> String {
    let status = status_name(&state.status);
    let current_plan = current_plan_text(state)
        .map(|plan| plan.to_string())
        .unwrap_or_else(|| {
            if language.starts_with("zh") {
                "当前没有已保存计划。".to_string()
            } else {
                "No current saved plan.".to_string()
            }
        });

    if language.starts_with("zh") {
        if state.mode == AgentPlanMode::Plan {
            format!(
                "Agent plan mode（内部运行模式）：当前模式是 plan，状态是 {status}。Plan mode 用于先调研、阅读、搜索、分析和提出计划；不要执行会产生副作用的动作，不要声称已经修改文件、运行命令、写入记忆或完成实现，除非 Kivio 返回了实际工具结果。可以提出必要的澄清问题。最终回复应给出可执行、简洁的计划，并说明需要用户切到 Act / 执行计划后才会实施。\n\n当前已保存计划：\n{current_plan}"
            )
        } else {
            format!(
                "Agent plan context（内部运行状态）：当前模式是 act，计划状态是 {status}。如果用户要求继续/执行计划，优先参考下面的已保存计划；若用户改变需求，以最新用户消息为准并说明计划需要调整。不要把 plan 当作用户可编辑 todo，也不要创建提醒或日历事项。\n\n当前已保存计划：\n{current_plan}"
            )
        }
    } else if state.mode == AgentPlanMode::Plan {
        format!(
            "Agent plan mode (internal runtime mode): current mode is plan and status is {status}. Plan mode is for researching, reading, searching, analyzing, asking clarifying questions, and producing a plan before action. Do not perform or claim side-effecting work such as editing files, running commands, mutating memory, or implementing changes unless Kivio returned an actual tool result. The final reply should be a concise executable plan and should make clear that implementation waits for Act / execute plan.\n\nCurrent saved plan:\n{current_plan}"
        )
    } else {
        format!(
            "Agent plan context (internal runtime state): current mode is act and plan status is {status}. If the user asks to continue or execute the plan, use the saved plan below as context; if the user changes requirements, follow the latest user message and note that the plan needs adjustment. Do not treat the plan as a user-editable todo list, and do not create reminders or calendar items.\n\nCurrent saved plan:\n{current_plan}"
        )
    }
}

pub fn current_plan_text(state: &AgentPlanState) -> Option<&str> {
    state
        .plan
        .as_deref()
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
}

fn status_name(status: &AgentPlanStatus) -> &'static str {
    match status {
        AgentPlanStatus::Empty => "empty",
        AgentPlanStatus::Draft => "draft",
        AgentPlanStatus::Approved => "approved",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_state_defaults_to_act_empty() {
        let state: AgentPlanState = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(state.mode, AgentPlanMode::Act);
        assert_eq!(state.status, AgentPlanStatus::Empty);
        assert_eq!(state.plan, None);
    }

    #[test]
    fn capture_draft_keeps_plan_mode_and_trims_reply() {
        let state =
            capture_draft_from_reply(&AgentPlanState::default(), "  1. Read code\n2. Edit  ");
        assert_eq!(state.mode, AgentPlanMode::Plan);
        assert_eq!(state.status, AgentPlanStatus::Draft);
        assert_eq!(state.plan.as_deref(), Some("1. Read code\n2. Edit"));
        assert!(state.updated_at > 0);
    }

    #[test]
    fn approve_without_plan_stays_empty_act() {
        let mut state = AgentPlanState::default();
        state.mode = AgentPlanMode::Plan;
        let approved = approve(&state);
        assert_eq!(approved.mode, AgentPlanMode::Act);
        assert_eq!(approved.status, AgentPlanStatus::Empty);
    }

    #[test]
    fn approve_with_plan_marks_approved() {
        let mut state = AgentPlanState::default();
        state.plan = Some("Plan".to_string());
        state.status = AgentPlanStatus::Draft;
        let approved = approve(&state);
        assert_eq!(approved.mode, AgentPlanMode::Act);
        assert_eq!(approved.status, AgentPlanStatus::Approved);
    }
}
