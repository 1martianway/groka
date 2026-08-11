# Announcement drafts (not posted)

Copy-ready drafts for LinkedIn and X. Review before publishing.  
Repo: https://github.com/1martianway/groka

**Title:** Groka — Grok Build with Auto Router  
**Description:** Open-source fork with auto effort router, usage limit bar, and more to come. Installs as groka beside official grok.

---

## LinkedIn (long-form)

**Suggested first line / hook (appears in feed):**  
Groka — Grok Build with Auto Router

---

**Groka — Grok Build with Auto Router**

Open-source fork with auto effort router, usage limit bar, and more to come. Installs as groka beside official grok.

I’m open-sourcing **groka**, a fork of xAI’s Grok Build with practical upgrades for everyday coding:

1. **Auto effort router** — picks `low` / `medium` / `high` reasoning effort **per turn** from your prompt (heuristic). Trivial turns don’t burn high effort; hard debug/coding still gets high. Pin with `/effort`, or leave auto. **No model swap.**

2. **Usage limit bar** — weekly/period Grok coding limit as a compact bar on the prompt chrome (left of the model chip). **Default on**; toggle with `/usage bar` or Settings.

3. **More to come** — thin, rebase-friendly fork. More session ergonomics and control on the way.

**What it is**  
Same Grok Build you already know — auth, models, tools, sessions — installed as `groka` next to official `grok`. Apache-2.0. Not an xAI product; a community fork that tracks upstream cleanly.

**Why**  
Long coding days mix trivial turns and hard ones. Defaulting everything to high effort (and burying limit state) is expensive and easy to ignore until it hurts. groka puts both on the surface.

**Try it**  
```
git clone https://github.com/1martianway/groka.git
cd groka && ./scripts/install-groka.sh
export PATH="$HOME/.local/bin:$PATH"
groka
```

Full story and config: https://github.com/1martianway/groka

Feedback and PRs welcome — keep patches thin so we can track upstream.

#OpenSource #AI #DeveloperTools #Grok #CLI #SoftwareEngineering

---

## X (Premium — long form OK)

**Mentions:** @elonmusk @SpaceXAI @grok  
**Hashtags:** #OpenSource #Grok #GrokBuild #CLI

### Primary (single post — recommended)

```
Groka — Grok Build with Auto Router

Open-source fork with auto effort router, usage limit bar, and more to come. Installs as groka beside official grok.

I'm open-sourcing groka, a thin Apache-2.0 fork of Grok Build:

1. Auto effort router — low / medium / high reasoning effort per turn from your prompt. Pin with /effort or leave auto. No model swap.

2. Usage limit bar — weekly/period coding limit on the prompt chrome (default on; /usage bar to toggle).

3. More to come — rebase-friendly patches; more session ergonomics on the way.

Same stack as official grok — install as groka next to it.

https://github.com/1martianway/groka

@elonmusk @SpaceXAI @grok

#OpenSource #Grok #GrokBuild #CLI
```

### Short variant (if you want tighter)

```
Groka — Grok Build with Auto Router

Open-source fork with auto effort router, usage limit bar, and more to come. Installs as groka beside official grok.

https://github.com/1martianway/groka

@elonmusk @SpaceXAI @grok

#OpenSource #Grok #GrokBuild #CLI
```

---

## Notes (not for the post)

- Title + description first; feature detail after.
- Tag xAI only if you want the reach; tone stays respectful (fork, not competitor).
- Do not auto-post; drafts only until you hit publish.
