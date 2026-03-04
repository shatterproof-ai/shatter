// Example 10: Cron expression parser
// Parses cron expressions into structured schedule descriptors.
// Exercises field-level validation, range/step/list parsing, and special
// string shortcuts.
//
// EXPECTED BRANCHES for ParseCron (22):
//   1. empty string                             → error: "empty expression"
//   2. special "@yearly" or "@annually"         → minute:0, hour:0, day:1, month:1, weekday:*
//   3. special "@monthly"                       → minute:0, hour:0, day:1, month:*, weekday:*
//   4. special "@weekly"                        → minute:0, hour:0, day:*, month:*, weekday:0
//   5. special "@daily" or "@midnight"          → minute:0, hour:0, day:*, month:*, weekday:*
//   6. special "@hourly"                        → minute:0, hour:*, day:*, month:*, weekday:*
//   7. unknown special string                   → error: "unknown special"
//   8. wrong number of fields (not 5)           → error: "expected 5 fields"
//   9. wildcard "*" in field                    → full range for that field
//  10. single number in range                   → exact value
//  11. number below field minimum               → error: "value below minimum"
//  12. number above field maximum               → error: "value above maximum"
//  13. range "a-b"                              → values from a to b inclusive
//  14. range with start > end                   → error: "invalid range"
//  15. step "*/n"                               → every nth value from min
//  16. step "a/n"                               → every nth value from a
//  17. step value zero                          → error: "step cannot be zero"
//  18. list "a,b,c"                             → union of values
//  19. list element invalid                     → error: "invalid value in list"
//  20. non-numeric value                        → error: "non-numeric value"
//  21. range within list "1,3-5,7"              → union includes range expansion
//  22. weekday field with 7 treated as 0 (Sunday) → normalized

package examples

import (
	"errors"
	"strconv"
	"strings"
)

// CronField holds the expanded set of values for one cron field.
type CronField struct {
	Values []int
}

// CronSchedule holds the parsed fields of a cron expression.
type CronSchedule struct {
	Minute     CronField
	Hour       CronField
	DayOfMonth CronField
	Month      CronField
	Weekday    CronField
}

type fieldBounds struct {
	min, max int
}

var cronFieldBounds = []fieldBounds{
	{0, 59},  // minute
	{0, 23},  // hour
	{1, 31},  // day of month
	{1, 12},  // month
	{0, 6},   // weekday (0=Sunday)
}

func makeRange(start, end int) []int {
	result := make([]int, 0, end-start+1)
	for i := start; i <= end; i++ {
		result = append(result, i)
	}
	return result
}

func allCronValues(min, max int) CronField {
	return CronField{Values: makeRange(min, max)}
}

func parseCronRange(expr string, min, max int) ([]int, error) {
	parts := strings.SplitN(expr, "-", 2)
	start, err := strconv.Atoi(parts[0])
	if err != nil {
		return nil, errors.New("non-numeric value")
	}
	end, err := strconv.Atoi(parts[1])
	if err != nil {
		return nil, errors.New("non-numeric value")
	}
	if start > end {
		return nil, errors.New("invalid range")
	}
	if start < min {
		return nil, errors.New("value below minimum")
	}
	if end > max {
		return nil, errors.New("value above maximum")
	}
	return makeRange(start, end), nil
}

func parseCronField(field string, min, max int) (CronField, error) {
	// Wildcard
	if field == "*" {
		return allCronValues(min, max), nil
	}

	// Step: */n or a/n
	if strings.Contains(field, "/") {
		parts := strings.SplitN(field, "/", 2)
		step, err := strconv.Atoi(parts[1])
		if err != nil {
			return CronField{}, errors.New("non-numeric value")
		}
		if step == 0 {
			return CronField{}, errors.New("step cannot be zero")
		}
		start := min
		if parts[0] != "*" {
			start, err = strconv.Atoi(parts[0])
			if err != nil {
				return CronField{}, errors.New("non-numeric value")
			}
		}
		values := make([]int, 0)
		for i := start; i <= max; i += step {
			values = append(values, i)
		}
		return CronField{Values: values}, nil
	}

	// List: a,b,c (may contain ranges)
	if strings.Contains(field, ",") {
		values := make([]int, 0)
		for _, part := range strings.Split(field, ",") {
			if strings.Contains(part, "-") {
				rangeVals, err := parseCronRange(part, min, max)
				if err != nil {
					return CronField{}, err
				}
				values = append(values, rangeVals...)
			} else {
				val, err := strconv.Atoi(part)
				if err != nil {
					return CronField{}, errors.New("invalid value in list")
				}
				if val < min {
					return CronField{}, errors.New("value below minimum")
				}
				if val > max {
					return CronField{}, errors.New("value above maximum")
				}
				values = append(values, val)
			}
		}
		return CronField{Values: values}, nil
	}

	// Range: a-b
	if strings.Contains(field, "-") {
		vals, err := parseCronRange(field, min, max)
		if err != nil {
			return CronField{}, err
		}
		return CronField{Values: vals}, nil
	}

	// Single number
	val, err := strconv.Atoi(field)
	if err != nil {
		return CronField{}, errors.New("non-numeric value")
	}
	if val < min {
		return CronField{}, errors.New("value below minimum")
	}
	if val > max {
		return CronField{}, errors.New("value above maximum")
	}
	return CronField{Values: []int{val}}, nil
}

// ParseCron parses a cron expression (5-field or special string) into a CronSchedule.
func ParseCron(expression string) (CronSchedule, error) {
	if len(expression) == 0 {
		return CronSchedule{}, errors.New("empty expression")
	}

	// Special strings
	if expression[0] == '@' {
		lower := strings.ToLower(expression)
		switch lower {
		case "@yearly", "@annually":
			return CronSchedule{
				Minute: CronField{Values: []int{0}}, Hour: CronField{Values: []int{0}},
				DayOfMonth: CronField{Values: []int{1}}, Month: CronField{Values: []int{1}},
				Weekday: allCronValues(0, 6),
			}, nil
		case "@monthly":
			return CronSchedule{
				Minute: CronField{Values: []int{0}}, Hour: CronField{Values: []int{0}},
				DayOfMonth: CronField{Values: []int{1}}, Month: allCronValues(1, 12),
				Weekday: allCronValues(0, 6),
			}, nil
		case "@weekly":
			return CronSchedule{
				Minute: CronField{Values: []int{0}}, Hour: CronField{Values: []int{0}},
				DayOfMonth: allCronValues(1, 31), Month: allCronValues(1, 12),
				Weekday: CronField{Values: []int{0}},
			}, nil
		case "@daily", "@midnight":
			return CronSchedule{
				Minute: CronField{Values: []int{0}}, Hour: CronField{Values: []int{0}},
				DayOfMonth: allCronValues(1, 31), Month: allCronValues(1, 12),
				Weekday: allCronValues(0, 6),
			}, nil
		case "@hourly":
			return CronSchedule{
				Minute: CronField{Values: []int{0}}, Hour: allCronValues(0, 23),
				DayOfMonth: allCronValues(1, 31), Month: allCronValues(1, 12),
				Weekday: allCronValues(0, 6),
			}, nil
		default:
			return CronSchedule{}, errors.New("unknown special")
		}
	}

	fields := strings.Fields(expression)
	if len(fields) != 5 {
		return CronSchedule{}, errors.New("expected 5 fields")
	}

	minute, err := parseCronField(fields[0], cronFieldBounds[0].min, cronFieldBounds[0].max)
	if err != nil {
		return CronSchedule{}, err
	}
	hour, err := parseCronField(fields[1], cronFieldBounds[1].min, cronFieldBounds[1].max)
	if err != nil {
		return CronSchedule{}, err
	}
	dayOfMonth, err := parseCronField(fields[2], cronFieldBounds[2].min, cronFieldBounds[2].max)
	if err != nil {
		return CronSchedule{}, err
	}
	month, err := parseCronField(fields[3], cronFieldBounds[3].min, cronFieldBounds[3].max)
	if err != nil {
		return CronSchedule{}, err
	}
	weekday, err := parseCronField(fields[4], cronFieldBounds[4].min, cronFieldBounds[4].max)
	if err != nil {
		return CronSchedule{}, err
	}

	// Normalize weekday 7 → 0 (Sunday)
	for i, v := range weekday.Values {
		if v == 7 {
			weekday.Values[i] = 0
		}
	}

	return CronSchedule{
		Minute:     minute,
		Hour:       hour,
		DayOfMonth: dayOfMonth,
		Month:      month,
		Weekday:    weekday,
	}, nil
}
