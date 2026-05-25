package nestedduration

import "time"

type WatcherConfig struct {
	Path     string
	Debounce time.Duration
}

func F(cfg struct{ Delay time.Duration }) int {
	if cfg.Delay < 0 {
		return -1
	}
	if cfg.Delay == 0 {
		return 0
	}
	return 1
}

func G(cfg WatcherConfig) int {
	if cfg.Debounce < 0 {
		return -1
	}
	if cfg.Debounce == 0 {
		return 0
	}
	if cfg.Debounce < time.Second {
		return 1
	}
	return 2
}
