// typepper on Railway: one service (HTTP + SSH) and its Postgres.
// `mise run infra:plan` / `infra:apply` / `infra:deploy` (see mise.toml);
// .github/workflows/deploy-production.yml runs apply then deploy.
// No GitHub source on purpose: Railway never deploys on its own, only CI does.
import { defineRailway, postgres, preserve, project, service } from "railway/iac";

export default defineRailway(() => {
  const db = postgres("Postgres"); // the one already in the project

  const server = service("typepper-server", {
    build: {
      builder: "DOCKERFILE",
      dockerfilePath: "infra/docker/typepper-server.Dockerfile",
    },
    // traffic moves to a new deploy only once this answers 200 (it checks Postgres)
    healthcheck: "/health",
    // IaC can't register custom domains: they were added with
    // `railway domain <name> -p 8080 -s typepper-server`, and are listed here so
    // applies keep them
    domains: [
      { domain: "api.typepper.com", port: 8080 },
      { domain: "r.typepper.com", port: 8080 },
    ],
    // ssh.typepper.com: Railway picks the public port, and released games keep
    // it, so this proxy must never be removed
    tcp: [2222],
    env: {
      PORT: "8080",
      SSH_PORT: "2222",
      DATABASE_URL: db.env.DATABASE_URL,
      // an OpenSSH private key, set once in Railway; the game ships its public half
      SSH_HOST_KEY: preserve(),
    },
  });

  return project("typepper", { resources: [server, db] });
});
