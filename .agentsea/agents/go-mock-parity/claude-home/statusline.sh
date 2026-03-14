#!/bin/bash
# Claude Code status line — shows session context and recent prompts
# Line 1: issue title (worktree) or first prompt after session start/clear
# Line 2: most recent user prompt (prefixed with issue key in worktrees)
input=$(cat)
DIR=$(echo "$input" | jq -r '.workspace.current_dir // empty')
TRANSCRIPT=$(echo "$input" | jq -r '.transcript_path // empty')
[ -z "$DIR" ] && exit 0

COLS=$(tput cols 2>/dev/null || echo 120)

# Truncate to width with ellipsis
trunc() {
    local str="$1" max="$2"
    if [ ${#str} -gt "$max" ]; then
        printf '%s…' "${str:0:$((max-1))}"
    else
        printf '%s' "$str"
    fi
}

# Strip XML-like tags and control text from prompt strings
clean_prompt() {
    sed -E 's/<[^>]+>//g; s/\[Request interrupted by user\]//g' | sed 's/^[[:space:]]*//' | head -1
}

# --- Determine if in a worktree and get issue info ---
IN_WORKTREE=false
ISSUE_KEY=""
ISSUE_TITLE=""
TOPLEVEL=$(git -C "$DIR" rev-parse --show-toplevel 2>/dev/null)
COMMON=$(git -C "$DIR" rev-parse --git-common-dir 2>/dev/null)
RESOLVED_COMMON=$(cd "$DIR" 2>/dev/null && realpath "$COMMON" 2>/dev/null)
if [ -n "$TOPLEVEL" ] && [ -n "$RESOLVED_COMMON" ]; then
    if [ "$TOPLEVEL/.git" != "$RESOLVED_COMMON" ]; then
        IN_WORKTREE=true
        ISSUE_KEY=$(basename "$TOPLEVEL")
        if JSON=$(bd show "$ISSUE_KEY" --json 2>/dev/null); then
            ISSUE_TITLE=$(echo "$JSON" | jq -r '.[0].title // empty')
        fi
    fi
fi

# --- Extract prompts from transcript via single jq pass ---
LINE1=""
LAST_PROMPT=""
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
    # jq filter: extract first and last user-typed prompts after the most recent
    # SessionStart or summary (which marks a /clear)
    # Outputs tab-separated: timestamp\tprompt_text
    PROMPTS=$(jq -rsf <(cat << 'JQEOF'
def extract_text:
  if .message.content | type == "array" then
    [.message.content[] | select(.type == "text") | .text][0]
  elif .message.content | type == "string" then
    .message.content
  else null end;

def find_start:
  last(to_entries[] |
    select(
      (.value.type == "progress" and .value.data.hookEvent == "SessionStart")
      or (.value.type == "summary")
    )
  ) | .key;

((find_start) // -1) as $start |

[to_entries[] | select(.key > $start) | .value |
  select(.type == "user") |
  { ts: .timestamp, text: (extract_text | select(. != null and . != "") | split("\n")[0]) } |
  select(.text != null)
] |
if length > 0 then
  "\(.[0].ts)\t\(.[0].text)", "\(.[-1].ts)\t\(.[-1].text)"
else empty end
JQEOF
    ) "$TRANSCRIPT" 2>/dev/null)

    FIRST_TS=$(echo "$PROMPTS" | head -1 | cut -f1 | xargs -I{} date -d {} +%H:%M 2>/dev/null)
    FIRST_PROMPT=$(echo "$PROMPTS" | head -1 | cut -f2- | clean_prompt)
    LAST_TS=$(echo "$PROMPTS" | tail -1 | cut -f1 | xargs -I{} date -d {} +%H:%M 2>/dev/null)
    RAW_LAST=$(echo "$PROMPTS" | tail -1 | cut -f2- | clean_prompt)

    # Build line 1 (with timestamp prefix)
    if [ "$IN_WORKTREE" = true ] && [ -n "$ISSUE_TITLE" ]; then
        LINE1="$ISSUE_KEY: $ISSUE_TITLE"
    elif [ -n "$FIRST_PROMPT" ]; then
        LINE1="${FIRST_TS:+$FIRST_TS }$FIRST_PROMPT"
    fi

    # Build line 2 (last prompt with timestamp, skip if same as line 1 content)
    if [ -n "$RAW_LAST" ] && [ "$RAW_LAST" != "$FIRST_PROMPT" ]; then
        if [ "$IN_WORKTREE" = true ] && [ -n "$ISSUE_KEY" ]; then
            LAST_PROMPT="${LAST_TS:+$LAST_TS }$ISSUE_KEY › $RAW_LAST"
        else
            LAST_PROMPT="${LAST_TS:+$LAST_TS }› $RAW_LAST"
        fi
    fi
fi

# --- Line 3: parallel work context (worktrees + unmerged branches) ---
LINE3=""
if [ -n "$TOPLEVEL" ]; then
    if [ "$IN_WORKTREE" = true ]; then
        # In a worktree: show .git/worktrees/<name> path so it's obvious
        WT_GIT_DIR=$(git -C "$DIR" rev-parse --git-dir 2>/dev/null)
        # WT_GIT_DIR is like /path/to/repo/.git/worktrees/<name>
        LINE3="${WT_GIT_DIR#"$RESOLVED_COMMON/"}"
    else
        # In main repo: show global worktree/branch overview
        WT_COUNT=$(git -C "$DIR" worktree list 2>/dev/null | wc -l)
        WT_COUNT=$((WT_COUNT - 1))  # exclude main worktree
        UNMERGED=$(git -C "$DIR" branch --no-merged main 2>/dev/null | wc -l)
        if [ "$WT_COUNT" -gt 0 ] || [ "$UNMERGED" -gt 0 ]; then
            CTX=""
            [ "$WT_COUNT" -gt 0 ] && CTX="$WT_COUNT worktree$([ $WT_COUNT -ne 1 ] && echo s)"
            if [ "$UNMERGED" -gt 0 ]; then
                # Find oldest unmerged branch age
                OLDEST=""
                while IFS= read -r branch; do
                    branch=$(echo "$branch" | sed 's/^[*+ ] //')
                    TS=$(git -C "$DIR" log -1 --format='%ct' "$branch" 2>/dev/null)
                    if [ -n "$TS" ]; then
                        if [ -z "$OLDEST" ] || [ "$TS" -lt "$OLDEST" ]; then
                            OLDEST=$TS
                        fi
                    fi
                done < <(git -C "$DIR" branch --no-merged main 2>/dev/null)
                LABEL="$UNMERGED unmerged branch$([ $UNMERGED -ne 1 ] && echo es)"
                if [ -n "$OLDEST" ]; then
                    NOW=$(date +%s)
                    AGE_S=$((NOW - OLDEST))
                    if [ "$AGE_S" -lt 3600 ]; then
                        AGE="$((AGE_S / 60))m ago"
                    elif [ "$AGE_S" -lt 86400 ]; then
                        AGE="$((AGE_S / 3600))h ago"
                    else
                        AGE="$((AGE_S / 86400))d ago"
                    fi
                    LABEL="$LABEL (oldest: $AGE)"
                fi
                [ -n "$CTX" ] && CTX="$CTX · "
                CTX="$CTX$LABEL"
            fi
            LINE3="$CTX"
        fi
    fi
fi

# --- Output ---
[ -z "$LINE1" ] && [ -z "$LAST_PROMPT" ] && [ -z "$LINE3" ] && exit 0

if [ -n "$LINE1" ]; then
    trunc "$LINE1" "$COLS"
    echo
fi

if [ -n "$LAST_PROMPT" ]; then
    trunc "$LAST_PROMPT" "$COLS"
    echo
fi

if [ -n "$LINE3" ]; then
    trunc "$LINE3" "$COLS"
    echo
fi
