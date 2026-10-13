#!/usr/bin/env node
// First-party non-model acceptance; no fixture event hooks or provider credentials.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, rm, statfs, writeFile } from "node:fs/promises";
import { createConnection } from "node:net";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const scratch = "/scratch";
const nonce = "forge-native-command-20261013";
const body = "Harmless first-party instructions. Body nonce: forge-body-20261013.";
const skillText = `---\nname: forge-probe\ndescription: Inert first-party activation fixture\n---\n${body}\n`;
const digest = (value) => createHash("sha256").update(value).digest("hex");
const emit = (stage, details = {}) => console.log(`FORGE_ACTIVATION_RECORD ${JSON.stringify({ stage, ...details })}`);
let stage = "isolation";

async function preflight() {
  assert.equal(process.platform, "linux");
  assert.notEqual(process.getuid(), 0);
  const allowed = new Set(["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME", "XDG_DATA_HOME", "PI_OFFLINE", "PI_CODING_AGENT_DIR", "GEMINI_CLI_HOME",
    "GEMINI_CLI_NO_RELAUNCH", "GEMINI_CLI_SYSTEM_SETTINGS_PATH", "GEMINI_CLI_SYSTEM_DEFAULTS_PATH"]);
  assert.deepEqual(Object.keys(process.env).filter((key) => !allowed.has(key)), []);
  for (const key of ["HOME", "TMPDIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
    "PI_CODING_AGENT_DIR", "GEMINI_CLI_HOME"]) assert.ok(process.env[key].startsWith(`${scratch}/`));
  const status = await readFile("/proc/self/status", "utf8");
  assert.match(status, /^NoNewPrivs:\s+1$/m);
  assert.match(status, /^CapEff:\s+0+$/m);
  const interfaces = await readdir("/sys/class/net");
  assert.deepEqual(interfaces, ["lo"]);
  await new Promise((resolve, reject) => {
    const socket = createConnection({ host: "10.255.255.1", port: 9 });
    socket.setTimeout(1500, () => { socket.destroy(); reject(new Error("Network denial timed out")); });
    socket.once("connect", () => { socket.destroy(); reject(new Error("Network unexpectedly available")); });
    socket.once("error", (error) => {
      socket.destroy();
      if (["ENETUNREACH", "EHOSTUNREACH", "EACCES", "EPERM"].includes(error.code)) resolve();
      else reject(new Error("Network denial was not established"));
    });
  });
  for (const path of ["/opt/forge/.write-canary", "/opt/pi/.write-canary"]) {
    await assert.rejects(writeFile(path, "inert", { flag: "wx" }), (error) => ["EROFS", "EACCES"].includes(error.code));
  }
  await assert.rejects(writeFile("/tmp/.write-canary", "inert", { flag: "wx" }), (error) => error.code === "EROFS");
  const mounts = await readFile("/proc/mounts", "utf8");
  const scratchMount = mounts.split("\n").find((line) => line.split(" ")[1] === scratch);
  assert.ok(scratchMount);
  const fields = scratchMount.split(" ");
  assert.equal(fields[2], "tmpfs");
  for (const flag of ["rw", "nosuid", "nodev", "noexec"]) assert.ok(fields[3].split(",").includes(flag));
  const fs = await statfs(scratch, { bigint: true });
  assert.equal(fs.bsize * fs.blocks, 128n * 1024n * 1024n);
  assert.equal((await readFile("/sys/fs/cgroup/memory.max", "utf8")).trim(), "1073741824");
  assert.equal((await readFile("/sys/fs/cgroup/memory.swap.max", "utf8")).trim(), "0");
  assert.equal((await readFile("/sys/fs/cgroup/pids.max", "utf8")).trim(), "64");
  const [quota, period] = (await readFile("/sys/fs/cgroup/cpu.max", "utf8")).trim().split(" ").map(Number);
  assert.equal(quota / period, 2);
  for (const path of ["home", "tmp", "config", "cache", "data", "pi-agent", "gemini-home", "workspace"]) {
    await mkdir(join(scratch, path), { recursive: true, mode: 0o700 });
  }
  assert.deepEqual(await readdir(process.env.HOME), []);
  await writeFile(join(scratch, "workspace", ".env"), "", { encoding: "utf8", flag: "wx" });
  await writeFile(join(scratch, "write-canary"), "scratch-only", { encoding: "utf8", flag: "wx" });
  assert.equal(await readFile(join(scratch, "write-canary"), "utf8"), "scratch-only");
  await rm(join(scratch, "write-canary"));
  emit("isolation-passed", { network: "none", root: "read-only", credentials: "empty", scratchMiB: 128 });
}

async function packageIdentity(root, name, version) {
  const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
  assert.equal(manifest.dependencies[name], version);
  const installed = JSON.parse(await readFile(join(root, "node_modules", name, "package.json"), "utf8"));
  assert.equal(installed.name, name);
  assert.equal(installed.version, version);
  const lock = JSON.parse(await readFile(join(root, "package-lock.json"), "utf8"));
  assert.equal(lock.packages[`node_modules/${name}`].version, version);
  assert.match(lock.packages[`node_modules/${name}`].resolved, /^https:\/\/registry\.npmjs\.org\//);
  assert.match(lock.packages[`node_modules/${name}`].integrity, /^sha512-/);
  return import(pathToFileURL(join(root, "node_modules", name, installed.main)).href);
}

async function piProbe() {
  const api = await packageIdentity("/opt/pi", "@earendil-works/pi-coding-agent", "1.2.0");
  const { DefaultResourceLoader, createAgentSession, SessionManager, SettingsManager, ModelRuntime } = api;
  const root = join(scratch, "workspace", "pi");
  const skill = join(root, "forge-probe", "SKILL.md");
  const marker = join(root, "command.json");
  const extension = join(root, "fixture.mjs");
  await mkdir(join(root, "forge-probe"), { recursive: true });
  await writeFile(skill, skillText, { encoding: "utf8", flag: "wx" });
  // Only a registered command: no timers, sockets, event hooks, custom tools or model calls.
  await writeFile(extension, `import {readFile, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
export default function(pi) {
  pi.registerCommand('forge-probe', {description:'Harmless fixture command', handler:async(args) => {
    if (args !== ${JSON.stringify(nonce)}) throw new Error('Fixture nonce rejected');
    const bytes=await readFile(${JSON.stringify(skill)});
    await writeFile(${JSON.stringify(marker)}, JSON.stringify({nonce:args,digest:createHash('sha256').update(bytes).digest('hex')}), {encoding:'utf8',flag:'wx'});
  }});
}
`, { encoding: "utf8", flag: "wx" });
  const agentDir = process.env.PI_CODING_AGENT_DIR;
  const settings = SettingsManager.inMemory({ packages: [], extensions: [], skills: [],
    enableSkillCommands: true, quietStartup: true });
  const runtime = await ModelRuntime.create({ modelsPath: null, refreshOnCreate: false, allowModelNetwork: false,
    authPath: join(agentDir, "auth.json") });
  assert.deepEqual([...runtime.getAvailableSnapshot()], []);
  const errors = [];
  const loader = new DefaultResourceLoader({ cwd: root, agentDir, settingsManager: settings,
    noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
    additionalExtensionPaths: [extension], additionalSkillPaths: [skill] });
  await loader.reload();
  assert.deepEqual(loader.getExtensions().errors, []);
  assert.equal(loader.getExtensions().extensions.length, 1);
  assert.deepEqual(loader.getSkills().diagnostics, []);
  const loaded = loader.getSkills().skills;
  assert.equal(loaded.length, 1);
  assert.equal(loaded[0].name, "forge-probe");
  assert.equal(loaded[0].filePath, skill);
  const { session } = await createAgentSession({ cwd: root, agentDir, resourceLoader: loader,
    modelRuntime: runtime, sessionManager: SessionManager.inMemory(root), settingsManager: settings, noTools: "all" });
  try {
    assert.equal(session.model, undefined);
    assert.deepEqual(session.getActiveToolNames(), []);
    await session.bindExtensions({ mode: "print", onError: (error) => errors.push(error) });
    await assert.rejects(readFile(marker), (error) => error.code === "ENOENT");
    await session.prompt(`/forge-probe ${nonce}`);
    assert.deepEqual(JSON.parse(await readFile(marker, "utf8")), { nonce, digest: digest(skillText) });
    assert.equal(errors.length, 0);
    await session.prompt("/forge-probe wrong-nonce");
    assert.equal(errors.length, 1);
    assert.equal(errors[0].event, "command");
    assert.equal(errors[0].error, "Fixture nonce rejected");
    await assert.rejects(session.prompt("/forge-unregistered"), /No model selected/);
    assert.equal(session.model, undefined);
    assert.deepEqual(JSON.parse(await readFile(marker, "utf8")), { nonce, digest: digest(skillText) });
  } finally {
    session.dispose();
  }
  const malformed = join(root, "invalid.mjs");
  await writeFile(malformed, "export default {invalidFactory:true};\n", { encoding: "utf8", flag: "wx" });
  const missingPath = join(root, "missing.mjs");
  const rejectedLoader = new DefaultResourceLoader({ cwd: root, agentDir, settingsManager: settings,
    noExtensions: true, noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
    additionalExtensionPaths: [missingPath, malformed] });
  await rejectedLoader.reload();
  assert.equal(rejectedLoader.getExtensions().extensions.length, 0);
  assert.ok(rejectedLoader.getExtensions().errors.some((error) => error.path === missingPath));
  assert.ok(rejectedLoader.getExtensions().errors.some((error) => error.path === malformed && /factory function/.test(error.error)));
  emit("pi-sdk-passed", { version: "1.2.0", callback: "native-command-dispatch", bodyDigest: digest(skillText),
    negatives: ["handler-error", "unregistered-command-no-model", "missing-extension", "invalid-factory"], modelCalls: 0 });
}

async function geminiProbe() {
  const { Config, ActivateSkillTool } = await packageIdentity("/opt/gemini", "@google/gemini-cli-core", "0.63.0");
  const root = join(scratch, "workspace", "gemini");
  const skill = join(root, ".agents", "skills", "forge-probe", "SKILL.md");
  const invalid = join(root, ".agents", "skills", "invalid", "SKILL.md");
  await mkdir(join(root, ".agents", "skills", "forge-probe"), { recursive: true });
  await mkdir(join(root, ".agents", "skills", "invalid"), { recursive: true });
  await writeFile(skill, skillText, { encoding: "utf8", flag: "wx" });
  await writeFile(invalid, "No frontmatter. This must not become a skill.", { encoding: "utf8", flag: "wx" });
  // Config constructs native components but does not initialize a client or acquire authentication.
  const config = new Config({ sessionId: nonce, targetDir: root, cwd: root, model: "",
    debugMode: false, telemetry: { enabled: false }, usageStatisticsEnabled: false,
    mcpEnabled: false, extensionsEnabled: false, extensionManagement: false, enableHooks: false,
    enableHooksUI: false, enableAgents: false, enableEventDrivenScheduler: false,
    fileFiltering: { enableFileWatcher: false }, skillsSupport: true, noBrowser: true,
    checkpointing: false });
  try {
    assert.equal(config.isInitialized(), false);
    const manager = config.getSkillManager();
    await manager.discoverSkills(config.storage, [], true);
    const nonbuiltin = manager.getSkills().filter((entry) => !entry.isBuiltin);
    assert.equal(nonbuiltin.length, 1);
    assert.equal(nonbuiltin[0].name, "forge-probe");
    assert.equal(nonbuiltin[0].location, skill);
    assert.equal(nonbuiltin[0].body, body);
    assert.equal(manager.getSkill("invalid"), null);
    const tool = new ActivateSkillTool(config, config.getMessageBus());
    assert.throws(() => tool.build({ name: "forge-unregistered" }));
    assert.throws(() => tool.build({ name: "" }));
    const invocation = tool.build({ name: "forge-probe" });
    assert.equal(manager.isSkillActive("forge-probe"), false);
    const result = await invocation.execute({ abortSignal: AbortSignal.timeout(5000) });
    assert.equal(result.error, undefined);
    assert.ok(result.llmContent.startsWith('<activated_skill name="forge-probe">'));
    assert.equal(result.llmContent.match(/<instructions>\s*([\s\S]*?)\s*<\/instructions>/)?.[1], body);
    assert.equal(manager.isSkillActive("forge-probe"), true);
    assert.ok(config.getWorkspaceContext().getDirectories().includes(join(root, ".agents", "skills", "forge-probe")));
    assert.equal(config.isInitialized(), false);
    manager.reset();
    const disappeared = tool.build({ name: "forge-probe" });
    manager.clearSkills();
    const rejected = await disappeared.execute({ abortSignal: AbortSignal.timeout(5000) });
    assert.ok(rejected.error);
    assert.match(rejected.error.message, /not found/);
    assert.equal(manager.isSkillActive("forge-probe"), false);
    emit("gemini-core-passed", { version: "0.63.0", consumption: "native-ActivateSkillTool", bodyDigest: digest(body),
      negatives: ["invalid-frontmatter", "unknown-enum", "empty-name", "disappeared-skill"], clientInitialized: false });
  } finally {
    await config.dispose();
  }
}

try {
  emit("isolation-start");
  await preflight();
  stage = "pi-sdk";
  emit("pi-sdk-start");
  await piProbe();
  stage = "gemini-core";
  emit("gemini-core-start");
  await geminiProbe();
  await rm(join(scratch, "workspace"), { recursive: true });
  assert.ok(!(await readdir(scratch)).includes("workspace"));
  emit("success", { surface: "pi-sdk-command-and-gemini-core-skill", modelInvocation: false, fullCliCoverage: false });
} catch (error) {
  // Never dump dependency stack traces or config/environment values to CI artifacts.
  const message = error instanceof Error ? error.message.slice(0, 1000)
    .replace(/(?:gh[pousr]_[A-Za-z0-9_]+|github_pat_[A-Za-z0-9_]+)/g, "[REDACTED]")
    .replace(/(authorization|api[_-]?key|token|password)\s*[:=]\s*\S+/gi, "$1=[REDACTED]") : "Unknown failure";
  emit("failed", { phase: stage, kind: error instanceof Error ? error.name : "UnknownError", message });
  process.exitCode = 1;
}
