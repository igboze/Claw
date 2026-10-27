import { defineConfig } from "drizzle-kit";

// Used only to diff the Drizzle schema against SQL migrations; new migrations are
// committed as plain SQL in ./migrations.
export default defineConfig({
  dialect: "postgresql",
  schema: "./src/schema.ts",
  out: "./migrations",
  dbCredentials: { url: process.env.DATABASE_URL ?? "postgres://claw:claw@localhost:5432/clawdao" },
});
