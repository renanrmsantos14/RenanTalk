// Sobe a versão de publicação (PATCH) e grava data/hora de São Paulo.
// Mantém package.json, src/version.json, tauri.conf.json e a versão da crate `renantalk` iguais.
// Versão fixa: argumento (`node bump-version.mjs 0.0.30`) ou RENANTALK_VERSAO. No release, o CI passa a versão da
// tag `v0.0.N` em RENANTALK_VERSAO; o `pnpm build` que o `tauri build` roda de novo regrava a mesma, sem subir o PATCH.
import { readFileSync, writeFileSync } from "node:fs";

const pkgUrl = new URL("../package.json", import.meta.url);
const pkg = JSON.parse(readFileSync(pkgUrl, "utf8"));
const [major, minor, patch] = pkg.version.split(".").map(Number);
const wanted = (process.argv[2] || process.env.RENANTALK_VERSAO)?.replace(/^v/, "");
if (wanted && !/^\d+\.\d+\.\d+$/.test(wanted)) throw new Error(`versão inválida: ${wanted}`);
pkg.version = wanted ?? `${major}.${minor}.${patch + 1}`;
writeFileSync(pkgUrl, JSON.stringify(pkg, null, 2) + "\n");

const builtAt = new Intl.DateTimeFormat("pt-BR", {
  timeZone: "America/Sao_Paulo",
  day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
}).format(new Date()).replace(",", "");
writeFileSync(new URL("../src/version.json", import.meta.url),
  JSON.stringify({ version: pkg.version, builtAt }, null, 2) + "\n");

const confUrl = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const conf = JSON.parse(readFileSync(confUrl, "utf8"));
conf.version = pkg.version;
writeFileSync(confUrl, JSON.stringify(conf, null, 2) + "\n");

const cargoUrl = new URL("../src-tauri/Cargo.toml", import.meta.url);
const cargo = readFileSync(cargoUrl, "utf8").replace(/^version = ".*"$/m, `version = "${pkg.version}"`);
writeFileSync(cargoUrl, cargo);
console.log(`RenanTalk v${pkg.version} · ${builtAt}`);
