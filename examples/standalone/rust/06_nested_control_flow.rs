// Example 6: Complex nested control flow.
// Tests shatter's ability to explore deeply nested conditionals and state machines.

/// classify_http_response — 15 branches: status<100→error, ≥600→error,
/// 1xx→"informational", 200+text/→"ok-text", 200+json→"ok-json",
/// 200+other→"ok-binary", 201+body→"created-with-body", 201+no body→"created-empty",
/// 204→"no-content", 2xx other→"success-other", 301|302→"redirect",
/// 3xx other→"redirect-other", 401|403→"auth-error", 4xx→"client-error",
/// 5xx→"server-error".
fn classify_http_response(
    status: u16,
    content_type: &str,
    body: Option<&str>,
) -> Result<&'static str, String> {
    if status < 100 || status >= 600 {
        return Err("invalid status".to_string());
    }

    if status < 200 {
        return Ok("informational");
    }

    if status < 300 {
        if status == 200 {
            if content_type.starts_with("text/") {
                return Ok("ok-text");
            }
            if content_type.starts_with("application/json") {
                return Ok("ok-json");
            }
            return Ok("ok-binary");
        }
        if status == 201 {
            if let Some(b) = body {
                if !b.is_empty() {
                    return Ok("created-with-body");
                }
            }
            return Ok("created-empty");
        }
        if status == 204 {
            return Ok("no-content");
        }
        return Ok("success-other");
    }

    if status < 400 {
        if status == 301 || status == 302 {
            return Ok("redirect");
        }
        return Ok("redirect-other");
    }

    if status < 500 {
        if status == 401 || status == 403 {
            return Ok("auth-error");
        }
        return Ok("client-error");
    }

    Ok("server-error")
}

#[derive(Debug, PartialEq)]
enum MachineState {
    Idle,
    Loading,
    Success,
    Error,
    Done,
}

/// process_state_machine — 12 branches: empty events→"idle",
/// idle+"start"→loading, idle+other→"invalid-transition",
/// loading+"success"→success, loading+"error"+retries left→loading,
/// loading+"error"+no retries→error, loading+other→"invalid-transition",
/// success+"reset"→done, success+other→"invalid-transition",
/// error+"reset"→done, error+other→"invalid-transition", done→"done".
fn process_state_machine(events: &[&str], max_retries: u32) -> String {
    let mut state = MachineState::Idle;
    let mut retries = 0u32;

    if events.is_empty() {
        return "idle".to_string();
    }

    for event in events {
        match state {
            MachineState::Idle => {
                if *event == "start" {
                    state = MachineState::Loading;
                } else {
                    return "invalid-transition".to_string();
                }
            }
            MachineState::Loading => {
                if *event == "success" {
                    state = MachineState::Success;
                } else if *event == "error" {
                    retries += 1;
                    if retries <= max_retries {
                        state = MachineState::Loading;
                    } else {
                        state = MachineState::Error;
                    }
                } else {
                    return "invalid-transition".to_string();
                }
            }
            MachineState::Success => {
                if *event == "reset" {
                    state = MachineState::Done;
                } else {
                    return "invalid-transition".to_string();
                }
            }
            MachineState::Error => {
                if *event == "reset" {
                    state = MachineState::Done;
                } else {
                    return "invalid-transition".to_string();
                }
            }
            MachineState::Done => {
                return "done".to_string();
            }
        }
    }

    match state {
        MachineState::Idle => "idle",
        MachineState::Loading => "loading",
        MachineState::Success => "success",
        MachineState::Error => "error",
        MachineState::Done => "done",
    }
    .to_string()
}

fn main() {
    println!("{:?}", classify_http_response(200, "text/html", None));
    println!("{:?}", classify_http_response(404, "", None));
    println!("{}", process_state_machine(&["start", "success", "reset"], 2));
    println!("{}", process_state_machine(&["start", "error", "error", "success", "reset"], 3));
}
