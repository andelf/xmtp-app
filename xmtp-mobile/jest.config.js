/** @type {import('jest').Config} */
module.exports = {
  testMatch: ["**/__tests__/**/*.(ts|tsx)", "**/*.(test|spec).(ts|tsx)"],
  setupFiles: ["./jest.setup.js"],
  transform: {
    "^.+\\.tsx?$": [
      "babel-jest",
      {
        presets: [
          ["@babel/preset-env", { targets: { node: "current" } }],
          "@babel/preset-typescript",
        ],
      },
    ],
  },
  testPathIgnorePatterns: ["/node_modules/"],
};
