const path = require("path");
const { getDefaultConfig } = require("expo/metro-config");

const projectRoot = __dirname;
const vendorMarkdownRoot = path.resolve(
  projectRoot,
  "../vendor/react-native-enriched-markdown"
);

const config = getDefaultConfig(projectRoot);

config.watchFolders = [...(config.watchFolders ?? []), vendorMarkdownRoot];
config.resolver.unstable_enableSymlinks = true;
config.resolver.nodeModulesPaths = [
  path.resolve(projectRoot, "node_modules"),
  path.resolve(vendorMarkdownRoot, "node_modules"),
];

module.exports = config;
