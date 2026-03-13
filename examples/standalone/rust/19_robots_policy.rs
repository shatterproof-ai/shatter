// Example 19: robots.txt rule evaluation.
// Adapted from samclarke/robots-parser (MIT): https://github.com/samclarke/robots-parser
// Directive parsing, user-agent grouping, wildcard path matching, and precedence.
//
// EXPECTED BRANCHES for evaluate_robots_policy (15):
//   1. empty user agent                          -> missing-user-agent
//   2. path missing leading slash                -> invalid-path
//   3. comments and blank lines ignored          -> parse continues
//   4. unknown directives ignored                -> parse continues
//   5. specific user-agent group beats wildcard  -> specific group selected
//   6. wildcard user-agent fallback              -> wildcard group selected
//   7. no matching group                         -> allow by default
//   8. empty disallow directive                  -> ignored
//   9. valid crawl-delay parsed                  -> crawl_delay populated
//  10. invalid crawl-delay ignored               -> crawl_delay absent
//  11. wildcard '*' in rule matches segment      -> rule applies
//  12. '$' end-anchor enforced                   -> suffix-sensitive match
//  13. longer allow beats shorter disallow       -> allowed
//  14. longer disallow beats allow               -> disallowed
//  15. equal-length tie resolved in favor allow  -> allowed
//
// DIFFICULTY: Hard. The solver must synthesize structured multi-line rule sets
// with interacting directives and carefully chosen paths.

#[derive(Clone)]
struct RobotsRule {
    directive: String,
    pattern: String,
}

#[derive(Clone)]
struct RobotsGroup {
    agents: Vec<String>,
    rules: Vec<RobotsRule>,
    crawl_delay: Option<f64>,
}

struct RobotsDecision {
    allowed: bool,
    reason: String,
    matched_directive: Option<String>,
    crawl_delay: Option<f64>,
}

fn normalize_robot_agent(agent: &str) -> String {
    let lower = agent.trim().to_ascii_lowercase();
    match lower.split_once('/') {
        Some((prefix, _)) => prefix.to_string(),
        None => lower,
    }
}

fn parse_robots_groups(robots_txt: &str) -> Vec<RobotsGroup> {
    let mut groups = Vec::new();
    let mut current: Option<RobotsGroup> = None;

    for raw_line in robots_txt.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let without_comment = raw_line.split('#').next().unwrap_or("").trim();
        if without_comment.is_empty() {
            continue;
        }

        let Some((key, value)) = without_comment.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        if key == "user-agent" {
            let needs_new_group = current
                .as_ref()
                .map(|group| !group.rules.is_empty() || group.crawl_delay.is_some())
                .unwrap_or(true);
            if needs_new_group {
                if let Some(group) = current.take() {
                    groups.push(group);
                }
                current = Some(RobotsGroup {
                    agents: Vec::new(),
                    rules: Vec::new(),
                    crawl_delay: None,
                });
            }
            if let Some(group) = current.as_mut() {
                group.agents.push(normalize_robot_agent(value));
            }
            continue;
        }

        let Some(group) = current.as_mut() else {
            continue;
        };

        match key.as_str() {
            "allow" | "disallow" => {
                if key == "disallow" && value.is_empty() {
                    continue;
                }
                group.rules.push(RobotsRule {
                    directive: key,
                    pattern: value.to_string(),
                });
            }
            "crawl-delay" => {
                if let Ok(delay) = value.parse::<f64>() {
                    if delay >= 0.0 {
                        group.crawl_delay = Some(delay);
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(group) = current {
        groups.push(group);
    }

    groups
}

fn group_matches_robot_agent(agent: &str, group: &RobotsGroup) -> bool {
    group
        .agents
        .iter()
        .any(|candidate| candidate != "*" && agent.starts_with(candidate))
}

fn matches_robot_pattern(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let body = if anchored {
        &pattern[..pattern.len() - 1]
    } else {
        pattern
    };

    fn walk(pattern: &[u8], path: &[u8]) -> bool {
        if pattern.is_empty() {
            return true;
        }
        if pattern[0] == b'*' {
            if walk(&pattern[1..], path) {
                return true;
            }
            if !path.is_empty() {
                return walk(pattern, &path[1..]);
            }
            return false;
        }
        if path.is_empty() || pattern[0] != path[0] {
            return false;
        }
        walk(&pattern[1..], &path[1..])
    }

    let matched = walk(body.as_bytes(), path.as_bytes());
    if !matched {
        return false;
    }
    if anchored {
        true
    } else {
        path.starts_with(body.split('*').next().unwrap_or("")) || body.contains('*')
    }
}

fn robot_rule_specificity(pattern: &str) -> usize {
    pattern
        .chars()
        .filter(|ch| *ch != '*' && *ch != '$')
        .count()
}

fn evaluate_robots_policy(robots_txt: &str, user_agent: &str, path: &str) -> RobotsDecision {
    let normalized_agent = normalize_robot_agent(user_agent);
    if normalized_agent.is_empty() {
        return RobotsDecision {
            allowed: false,
            reason: "missing-user-agent".to_string(),
            matched_directive: None,
            crawl_delay: None,
        };
    }
    if !path.starts_with('/') {
        return RobotsDecision {
            allowed: false,
            reason: "invalid-path".to_string(),
            matched_directive: None,
            crawl_delay: None,
        };
    }

    let groups = parse_robots_groups(robots_txt);
    let specific_groups: Vec<RobotsGroup> = groups
        .iter()
        .filter(|group| group_matches_robot_agent(&normalized_agent, group))
        .cloned()
        .collect();
    let wildcard_groups: Vec<RobotsGroup> = groups
        .iter()
        .filter(|group| group.agents.iter().any(|candidate| candidate == "*"))
        .cloned()
        .collect();
    let selected_groups = if !specific_groups.is_empty() {
        specific_groups
    } else {
        wildcard_groups
    };

    if selected_groups.is_empty() {
        return RobotsDecision {
            allowed: true,
            reason: "no-group".to_string(),
            matched_directive: None,
            crawl_delay: None,
        };
    }

    let mut crawl_delay = None;
    let mut best_rule: Option<RobotsRule> = None;
    let mut best_specificity = 0usize;

    for group in &selected_groups {
        if crawl_delay.is_none() {
            crawl_delay = group.crawl_delay;
        }
        for rule in &group.rules {
            if !matches_robot_pattern(&rule.pattern, path) {
                continue;
            }
            let specificity = robot_rule_specificity(&rule.pattern);
            let better_directive = best_rule
                .as_ref()
                .map(|current| {
                    specificity == best_specificity
                        && rule.directive == "allow"
                        && current.directive == "disallow"
                })
                .unwrap_or(false);
            if best_rule.is_none() || specificity > best_specificity || better_directive {
                best_rule = Some(rule.clone());
                best_specificity = specificity;
            }
        }
    }

    match best_rule {
        Some(rule) => RobotsDecision {
            allowed: rule.directive == "allow",
            reason: rule.directive.clone(),
            matched_directive: Some(format!("{}:{}", rule.directive, rule.pattern)),
            crawl_delay,
        },
        None => RobotsDecision {
            allowed: true,
            reason: "no-rule".to_string(),
            matched_directive: None,
            crawl_delay,
        },
    }
}

fn main() {
    let robots = "User-agent: *\nDisallow: /private/\nAllow: /private/public$\nCrawl-delay: 2";
    let result = evaluate_robots_policy(robots, "DocsBot/1.0", "/private/public");
    println!(
        "allowed={} reason={} matched={:?} delay={:?}",
        result.allowed, result.reason, result.matched_directive, result.crawl_delay
    );
}
