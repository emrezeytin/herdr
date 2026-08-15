use crate::api::schema::{
    Request, WorkspaceCreateParams, WorkspaceSetGoalParams, WorkspaceSetSettledParams,
    WorktreeCreateParams,
};

/// Product session commands. A session is a named unit of work (a workspace
/// with a goal), optionally isolated in a linked git worktree.
pub(super) fn run_session_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_session_help();
        return Ok(2);
    };

    match subcommand {
        "create" => session_create(&args[1..]),
        "list" => session_list(&args[1..]),
        "focus" => session_focus(&args[1..]),
        "goal" => session_goal(&args[1..]),
        "settle" => session_set_settled(&args[1..], true),
        "unsettle" => session_set_settled(&args[1..], false),
        // Legacy aliases for the pre-fork background-session commands, now
        // under `instance`.
        "attach" | "stop" | "delete" => super::run_instance_command(args),
        "help" | "--help" | "-h" => {
            print_session_help();
            Ok(0)
        }
        _ => {
            print_session_help();
            Ok(2)
        }
    }
}

fn session_create(args: &[String]) -> std::io::Result<i32> {
    let mut cwd = None;
    let mut label = None;
    let mut goal = None;
    let mut focus = false;
    let mut worktree = false;
    let mut branch = None;
    let mut base = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--cwd" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --cwd");
                    return Ok(2);
                };
                cwd = Some(value.clone());
                index += 2;
            }
            "--label" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --label");
                    return Ok(2);
                };
                label = Some(value.clone());
                index += 2;
            }
            "--goal" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --goal");
                    return Ok(2);
                };
                goal = Some(value.clone());
                index += 2;
            }
            "--worktree" => {
                worktree = true;
                index += 1;
            }
            "--branch" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --branch");
                    return Ok(2);
                };
                branch = Some(value.clone());
                index += 2;
            }
            "--base" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --base");
                    return Ok(2);
                };
                base = Some(value.clone());
                index += 2;
            }
            "--focus" => {
                focus = true;
                index += 1;
            }
            "--no-focus" => {
                focus = false;
                index += 1;
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    if worktree {
        if branch.is_none() && base.is_none() {
            eprintln!("--worktree requires --branch or --base");
            return Ok(2);
        }
        let response = super::send_request(&Request {
            id: "cli:session:create:worktree".into(),
            method: crate::api::schema::Method::WorktreeCreate(WorktreeCreateParams {
                workspace_id: None,
                cwd: cwd.clone(),
                branch,
                base,
                path: None,
                label: label.clone(),
                focus,
            }),
        })?;
        let workspace_id = response
            .get("result")
            .and_then(|result| result.get("workspace"))
            .and_then(|workspace| workspace.get("workspace_id"))
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        if let Some(workspace_id) = workspace_id {
            if let Some(goal) = goal {
                super::send_request(&Request {
                    id: "cli:session:create:goal".into(),
                    method: crate::api::schema::Method::WorkspaceSetGoal(WorkspaceSetGoalParams {
                        workspace_id,
                        goal,
                    }),
                })?;
            }
        }
        return super::print_response(&response);
    }

    super::runtime::workspace_create(WorkspaceCreateParams {
        cwd,
        focus,
        label,
        goal,
        env: Default::default(),
    })
}

fn session_list(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: sessionr session list");
        return Ok(2);
    }

    super::runtime::workspace_list()
}

fn session_focus(args: &[String]) -> std::io::Result<i32> {
    let Some(raw_workspace_id) = args.first() else {
        eprintln!("usage: sessionr session focus <session_id>");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("usage: sessionr session focus <session_id>");
        return Ok(2);
    }

    super::runtime::workspace_focus(super::normalize_workspace_id(raw_workspace_id))
}

fn session_goal(args: &[String]) -> std::io::Result<i32> {
    let (Some(raw_workspace_id), Some(goal)) = (args.first(), args.get(1)) else {
        eprintln!("usage: sessionr session goal <session_id> <text>");
        return Ok(2);
    };
    if args.len() != 2 || goal.is_empty() {
        eprintln!("usage: sessionr session goal <session_id> <text>");
        return Ok(2);
    }

    super::runtime::workspace_set_goal(WorkspaceSetGoalParams {
        workspace_id: super::normalize_workspace_id(raw_workspace_id),
        goal: goal.clone(),
    })
}

fn session_set_settled(args: &[String], settled: bool) -> std::io::Result<i32> {
    let Some(raw_workspace_id) = args.first() else {
        eprintln!(
            "usage: sessionr session {} <session_id>",
            if settled { "settle" } else { "unsettle" }
        );
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!(
            "usage: sessionr session {} <session_id>",
            if settled { "settle" } else { "unsettle" }
        );
        return Ok(2);
    }

    super::runtime::workspace_set_settled(WorkspaceSetSettledParams {
        workspace_id: super::normalize_workspace_id(raw_workspace_id),
        settled,
    })
}

fn print_session_help() {
    eprintln!("sessionr session commands:");
    eprintln!("  sessionr session create [--cwd PATH] [--goal TEXT] [--label TEXT] [--worktree] [--branch NAME] [--base REF] [--focus]");
    eprintln!("  sessionr session list");
    eprintln!("  sessionr session focus <session_id>");
    eprintln!("  sessionr session goal <session_id> <text>");
    eprintln!("  sessionr session settle <session_id>");
    eprintln!("  sessionr session unsettle <session_id>");
}
