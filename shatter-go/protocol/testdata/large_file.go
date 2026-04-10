// Package testdata provides test fixtures for large file analysis
package testdata

// LargeConfig represents a very large configuration struct with many fields
// This is designed to test analyze response truncation with large type information
type LargeConfig struct {
	// Columns at various positions to trigger truncation at ~9000-26000 chars
	Field001 string
	Field002 string
	Field003 string
	Field004 string
	Field005 string
	Field006 string
	Field007 string
	Field008 string
	Field009 string
	Field010 string
	Field011 string
	Field012 string
	Field013 string
	Field014 string
	Field015 string
	Field016 string
	Field017 string
	Field018 string
	Field019 string
	Field020 string
	Field021 string
	Field022 string
	Field023 string
	Field024 string
	Field025 string
	Field026 string
	Field027 string
	Field028 string
	Field029 string
	Field030 string
	Field031 string
	Field032 string
	Field033 string
	Field034 string
	Field035 string
	Field036 string
	Field037 string
	Field038 string
	Field039 string
	Field040 string
}

// FunctionWithManyParams has many parameters to create large analyze response
func FunctionWithManyParams(
	config LargeConfig,
	param1 string,
	param2 int,
	param3 float64,
	param4 bool,
	param5 string,
	param6 int,
	param7 float64,
	param8 bool,
	param9 string,
	param10 int,
	param11 float64,
	param12 bool,
	param13 string,
	param14 int,
	param15 float64,
	param16 bool,
	param17 string,
	param18 int,
	param19 float64,
	param20 bool,
) (LargeConfig, error) {
	return config, nil
}
