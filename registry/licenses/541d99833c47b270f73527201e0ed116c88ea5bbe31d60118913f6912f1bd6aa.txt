# AGIRAILS Payments — OpenClaw Skill

Give your [OpenClaw](https://openclaw.ai) AI agent a wallet. Let it earn and pay USDC — settled on-chain, gasless, in under 2 seconds.

## What It Does

This skill integrates [AGIRAILS](https://agirails.io) into any OpenClaw/Clawdbot agent, enabling:

- **ACTP escrow** — lock USDC, do the work, deliver, settle
- **x402 instant payments** — pay-per-API-call, no escrow
- **ERC-8004 identity** — portable on-chain reputation
- **Gasless transactions** — gas sponsored on Base L2
- **Encrypted keystore** — auto-generated, AES-128-CTR, no keys in code

## Quick Start

```bash
# Install the skill
git clone https://github.com/agirails/openclaw-skill.git ~/.openclaw/skills/agirails

# Initialize (mock mode — 10,000 test USDC)
npx actp init --mode mock
```

Then tell your agent:

> "Pay 10 USDC to 0xProviderAddress for translation service"

## Networks

| | Mock | Testnet (Base Sepolia) | Mainnet (Base) |
|---|---|---|---|
| **Cost** | Free | Free (1,000 USDC on registration) | Real USDC |
| **Gas** | Simulated | Sponsored | Sponsored |
| **Tx limit** | None | None | $1,000 |

## Structure

```
agirails/
├── SKILL.md              # Full protocol reference
├── README.md             # Detailed guide
├── references/           # State machine, agent templates
├── examples/             # Payment examples (all 3 API tiers)
├── openclaw/             # OpenClaw-specific configs & SOULs
│   ├── QUICKSTART.md     # 5-minute setup
│   ├── SOUL-*.md         # Agent templates (buyer, seller, autonomous)
│   └── security-checklist.md
└── scripts/              # Setup & test scripts
```

## Links

- [Documentation](https://docs.agirails.io/guides/integrations/openclaw)
- [SDK (npm)](https://www.npmjs.com/package/@agirails/sdk) | [SDK (pip)](https://pypi.org/project/agirails/)
- [Discord](https://discord.gg/nuhCt75qe4) | [GitHub](https://github.com/agirails)

## License

MIT
