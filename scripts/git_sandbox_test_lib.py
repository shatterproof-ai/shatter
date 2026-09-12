"""Fixture-facing aliases for the existing Git environment sanitizer.

Pass sanitized_git_env() to every subprocess that targets a disposable
repository, including its first init and all helper commands. See str-jttrf.
"""

from scripts.examples_checkout import GIT_LOCAL_ENV_VARS as GIT_SANDBOX_ENV_VARS
from scripts.examples_checkout import _sanitized_git_env as sanitized_git_env
