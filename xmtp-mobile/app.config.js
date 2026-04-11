const { execSync } = require("child_process");

let gitCommit = "unknown";
try {
  gitCommit = execSync("git rev-parse --short HEAD").toString().trim();
} catch (_) {
  // git not available in CI or sandboxed environment
}

module.exports = ({ config }) => ({
  ...config,
  extra: {
    ...config.extra,
    gitCommit,
    buildTime: new Date().toISOString(),
  },
});
