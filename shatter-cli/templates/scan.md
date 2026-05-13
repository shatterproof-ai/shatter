# Scan Results

{% if !sampling_info.is_empty() -%}
{{ sampling_info }}

{% endif -%}
| Function | Paths | Coverage |
|---|---|---|
{% for f in functions -%}
| `{{ f.function_name }}` | {{ f.unique_paths }} | {{ f.coverage }} |
{% endfor %}
---

Scan complete: **{{ total_tested }} completed**, **{{ skipped_errors.len() }} failed**, **{{ unsupported_count }} unsupported**, **{{ skipped_expected.len() }} skipped** ({{ workers_used }} worker(s))
{% if !skipped_errors.is_empty() %}
**Errors:**
{% for s in skipped_errors -%}
- `{{ s.function_name }}`: {{ s.reason }}
{% endfor -%}
{% endif -%}
{% if !skipped_expected.is_empty() %}
**Skipped:**
{% for s in skipped_expected -%}
- `{{ s.function_name }}`: {{ s.reason }}
{% endfor -%}
{% endif -%}