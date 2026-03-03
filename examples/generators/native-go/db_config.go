// Package usergens provides example native generators for Go custom builds.
//
// Usage: place this file at .shatter/generators/db_config.go and reference it
// in config.yaml:
//
//	defaults:
//	  generators:
//	    DbConfig: .shatter/generators/db_config.go
//
// Then build: shatter build-frontend go
package usergens

import (
	"encoding/json"
	"fmt"
	"math/rand"

	"github.com/shatter-dev/shatter/shatter-go/generators"
)

// DbConfig generates database configuration objects. On fresh generation
// (recipe == nil), it creates a randomized test database name. On replay,
// it reconstructs from the stored recipe.
func DbConfig(recipe json.RawMessage) generators.NativeGeneratorResult {
	type Config struct {
		Host string `json:"host"`
		Port int    `json:"port"`
		DB   string `json:"db"`
	}

	var cfg Config
	if recipe != nil {
		_ = json.Unmarshal(recipe, &cfg)
	} else {
		cfg = Config{
			Host: "localhost",
			Port: 5432,
			DB:   fmt.Sprintf("test_%04d", rand.Intn(10000)),
		}
	}

	recipeBytes, _ := json.Marshal(cfg)
	return generators.NativeGeneratorResult{
		ID:     "local-postgres",
		Value:  cfg, // live object, stays in-process
		Recipe: recipeBytes,
	}
}
