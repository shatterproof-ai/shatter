## `{{ function_name }}`{{ location }}

{{ summary }}

{% if !paths.is_empty() -%}
| # | Call | Outcome |
|---|---|---|
{% for path in paths -%}
| {{ path.index }} | `{{ path.call }}` | {{ path.outcome }} |
{% endfor -%}
{% endif -%}
{% for extra in extras -%}
- *{{ extra }}*
{% endfor -%}