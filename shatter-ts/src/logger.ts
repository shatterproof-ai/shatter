import pino from "pino";

const VALID_LEVELS: Record<string, string> = {
  error: "error",
  warn: "warn",
  info: "info",
  debug: "debug",
  trace: "trace",
};

const envLevel = process.env["SHATTER_LOG_LEVEL"]?.toLowerCase() ?? "info";
const level = VALID_LEVELS[envLevel] ?? "info";

const logger = pino({
  level,
  transport: {
    target: "pino/file",
    options: { destination: 2 },
  },
  msgPrefix: "[shatter-ts] ",
});

export default logger;
