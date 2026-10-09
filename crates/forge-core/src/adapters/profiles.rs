use crate::model::{Capability, Kind};

pub(super) struct Profile {
    pub target: &'static str,
    pub surface: &'static str,
    pub delivery: &'static str,
    pub version: &'static str,
    pub docs: &'static str,
    pub kinds: Vec<Kind>,
    pub restriction: &'static str,
}

pub(super) fn profiles() -> Vec<Profile> {
    use Kind::*;
    let mut rows = vec![];
    let mut add = |target, surface, delivery, version, docs, kinds, restriction| {
        rows.push(Profile {
            target,
            surface,
            delivery,
            version,
            docs,
            kinds,
            restriction,
        });
    };
    add(
        "claude-code",
        "cli",
        "native-plugin",
        "2.1.295",
        "https://code.claude.com/docs/en/plugins-reference",
        vec![
            Skills,
            Assets,
            Plugins,
            Agents,
            Commands,
            Hooks,
            Mcps,
            Lsp,
            OutputStyles,
            Workflows,
            Themes,
            Settings,
        ],
        "Native component identity required; hooks merge, skills add, other explicit paths replace defaults; no duplicate hook declaration",
    );
    add(
        "codex",
        "cli",
        "native-plugin",
        "0.162.0",
        "https://developers.openai.com/codex/plugins/build/",
        vec![Skills, Assets, Plugins, Hooks, Mcps],
        "Compatibility manifest only; portable stable loader skips hooks/apps; agent-role packaging unsupported; desktop hook activation remains host-specific",
    );
    add(
        "hermes",
        "cli",
        "portable-plugin",
        "0.21.6",
        "https://hermes-agent.nousresearch.com/docs/developer-guide/plugins/",
        vec![Skills, Assets, Plugins, Mcps],
        "Agent Plugins 1.0.0 skills/MCP subset; SSE unsupported; install disabled until explicit enablement; native Python uses hermes-native",
    );
    add(
        "hermes-native",
        "cli",
        "native-plugin",
        "0.21.6",
        "https://hermes-agent.nousresearch.com/docs/developer-guide/plugins/",
        vec![Plugins, Assets],
        "Preserve plugin.yaml and __init__.py; never synthesize register(ctx), import Python, install dependencies or run doctor",
    );
    add(
        "cursor",
        "desktop",
        "native-plugin",
        "docs-2026-10-09",
        "https://cursor.com/docs/reference/plugins",
        vec![
            Skills, Assets, Plugins, Rules, Agents, Commands, Hooks, Mcps,
        ],
        "Native Cursor identity required; component paths replace defaults; standard PLUGIN_ROOT/PLUGIN_DATA tokens not expanded; exact host version unknown",
    );
    add(
        "antigravity",
        "desktop",
        "native-plugin",
        "docs-v1",
        "https://antigravity.google/docs/plugins",
        vec![Skills, Assets, Plugins, Agents, Rules, Hooks, Mcps],
        "Root Antigravity manifest is not Agent Plugins; named hooks and MCP serverUrl remain native; schema endpoint unavailable; host version unknown",
    );
    for target in ["opencode", "opencode-v1"] {
        add(
            target,
            "cli",
            "native-config",
            "1.18.35",
            "https://opencode.ai/docs/plugins/",
            vec![Skills, Assets, Plugins],
            "V1 JS/TS plugins and plugin singular configuration; native code preserved only; no automatic Bun installs; no v2 API conversion",
        );
    }
    add(
        "opencode-v2",
        "cli",
        "native-config",
        "2.0.24",
        "https://github.com/anomalyco/opencode/blob/v2.0.24/packages/core/src/config/plugin/source.ts",
        vec![Skills, Assets, Plugins],
        "V2 local plugins must be directories; plugins plural and @opencode/plugin; preserve module package, never generate setup or execute it",
    );
    add(
        "copilot",
        "cli",
        "portable-plugin",
        "1.0.94",
        "https://docs.github.com/en/copilot/reference/cli-plugin-reference",
        vec![Skills, Assets, Plugins, Mcps],
        "Agent Plugins 1.0.0 subset only; vendor runtime components refused; hook payload/timeouts not exhaustively verified",
    );
    add(
        "vscode",
        "desktop-local",
        "portable-plugin",
        "docs-2026-10-07",
        "https://code.visualstudio.com/docs/copilot/customization/agent-plugins",
        vec![Skills, Assets, Plugins, Mcps],
        "Agent plugin, not VSIX; local harness ignores matcher values; executable hook conversion refused; exact stable host version unknown",
    );
    add(
        "gemini",
        "cli",
        "native-plugin",
        "0.63.0",
        "https://geminicli.com/docs/extensions/reference/",
        vec![
            Skills, Assets, Plugins, Commands, Agents, Hooks, Rules, Mcps,
        ],
        "Commands and policies must be TOML; agents preview; hooks fixed hooks/hooks.json; MCP merged into extension manifest without granting trust",
    );
    add(
        "amp",
        "cli",
        "native-config",
        "docs-2026-10-06",
        "https://ampcode.com/docs/markdown/customize/skills",
        vec![Skills, Assets],
        "Standalone skill contribution only; native module bundles use amp-native; host version and disposal/timeouts unknown",
    );
    add(
        "amp-native",
        "cli",
        "native-config",
        "docs-2026-10-06",
        "https://ampcode.com/docs/markdown/customize/plugins",
        vec![Plugins, Assets],
        "Preserve default-function JS/TS module; directory index.ts/index.js required; registerSkill not synthesized; no session.end contract",
    );
    add(
        "pi",
        "cli",
        "native-plugin",
        "1.1.0",
        "https://github.com/earendil-works/pi/blob/v1.1.0/packages/coding-agent/docs/packages.md",
        vec![Skills, Assets, Plugins, Mods, Prompts, Themes],
        "Explicit pi resource paths; preserve extensions; host peer namespace @earendil-works; no dependency installs, globs or hook conversion",
    );
    add(
        "kiro",
        "desktop",
        "portable-plugin",
        "agent-plugins-1.0.0",
        "https://kiro.dev/docs/powers/create/",
        vec![Skills, Assets, Plugins, Mcps],
        "Power skills/MCP subset only, not arbitrary executable runtime; managed import; exact installed power path unknown; assembler attribution is not source authorship",
    );
    add(
        "windsurf",
        "desktop-cascade",
        "native-config",
        "docs-2026-10-09",
        "https://docs.windsurf.com/windsurf/cascade/skills",
        vec![Skills, Assets, Mcps],
        "Cascade only; Devin Local contract unknown; MCP is an uninstalled config contribution, not native plugin",
    );
    add(
        "cline",
        "cli",
        "native-config",
        "docs-2026-10-09",
        "https://docs.cline.bot/mcp/configuring-mcp-servers",
        vec![Skills, Assets, Mcps],
        "Skills/MCP config only; native runtime plugin packaging unverified; remote streamableHttp type preserved; IDE config destination user-selected",
    );
    add(
        "kilo",
        "desktop",
        "native-config",
        "docs-2026-10-09",
        "https://kilo.ai/docs/customize/skills",
        vec![Skills, Assets],
        "Skill subset only; native plugin release/module/lifecycle unverified and docs/source schemas conflict",
    );
    add(
        "continue",
        "cli",
        "native-config",
        "source-5522c6f",
        "https://docs.continue.dev/customize/deep-dives/mcp",
        vec![Skills, Assets, Mcps],
        "Skills and project MCP JSON contributions only; native runtime plugin/hook lifecycle and IDE parity unverified",
    );
    add(
        "openhands",
        "sdk",
        "portable-plugin",
        "sdk-1.49.2",
        "https://docs.openhands.dev/overview/agent-plugins.md",
        vec![Skills, Assets, Plugins, Mcps],
        "Agent Plugins 1.0.0 skills/MCP subset; SSE skipped; host hooks require native OpenHands identity, not Claude event resemblance",
    );
    add(
        "openclaw",
        "cli",
        "portable-plugin",
        "docs-2026-10-09",
        "https://docs.openclaw.ai/plugins/manifest",
        vec![Skills, Assets, Plugins],
        "Compatible skill bundle subset only; MCP/hook bundle subset unverified; native executable packages use openclaw-native; host version unknown",
    );
    add(
        "openclaw-native",
        "cli",
        "native-plugin",
        "experimental",
        "https://docs.openclaw.ai/plugins/manifest",
        vec![Plugins, Assets],
        "Preserve openclaw.plugin.json plus package.json openclaw.extensions; entry files required; experimental host API version must be pinned; no runtime imports",
    );
    rows
}

pub(super) fn capability(profile: Profile) -> Capability {
    Capability {
        harness: profile.target.into(), surface: profile.surface.into(),
        profile: format!("{}-{}", profile.target, profile.version), delivery: profile.delivery.into(),
        kinds: profile.kinds, documentation: profile.docs.into(),
        verification: "static-contract; real-host-not-tested".into(),
        restrictions: vec![profile.restriction.into(), "Non-skill components require matching native identity; no universal conversion, code execution, dependency installs or user enablement".into()],
    }
}
