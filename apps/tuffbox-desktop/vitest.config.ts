import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Two projects: node (default, pure logic) and a happy-dom project for
    // tests that touch localStorage (libraryGroups persistence).
    projects: [
      {
        test: {
          name: "node",
          include: ["src/**/*.test.ts"],
          exclude: ["src/lib/libraryGroups.test.ts"],
        },
      },
      {
        test: {
          name: "dom",
          include: ["src/lib/libraryGroups.test.ts"],
          environment: "happy-dom",
        },
      },
    ],
  },
});
