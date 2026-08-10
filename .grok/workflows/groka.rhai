let meta = #{
    name: "groka",
    description: "Release-build 1martianway/groka and install ~/.local/bin/groka with version = git HEAD",
    when_to_use: "Rebuild/install the groka fork after pulling or changing code so groka --version matches HEAD",
    phases: [
        #{ title: "TelemetryStart", detail: "snapshot usage proxies" },
        #{ title: "BuildInstall", detail: "install-groka.sh (build + stamp + install)" },
        #{ title: "TelemetryEnd", detail: "snapshot after" },
    ],
};

let result_schema = #{
    "type": "object",
    "required": ["ok", "summary", "command", "log_tail", "head", "version_line"],
    "properties": #{
        "ok": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "command": #{ "type": "string" },
        "log_tail": #{ "type": "string" },
        "head": #{ "type": "string" },
        "version_line": #{ "type": "string" },
        "binary_path": #{ "type": "string" },
    },
};

let tele_schema = #{
    "type": "object",
    "required": ["ok", "detail"],
    "properties": #{
        "ok": #{ "type": "boolean" },
        "detail": #{ "type": "string" },
    },
};

// args.repo: override repo root (default ~/repos/groka)
// args.skip_build: true → install existing release only (stale stamp risk)
let repo = "/Users/karan/repos/groka";
let skip_build = false;
if args != () {
    if args.repo != () { repo = args.repo; }
    if args.skip_build != () { skip_build = args.skip_build; }
}

let cmd = "cd " + repo + " && ./scripts/install-groka.sh";
if skip_build {
    cmd = "cd " + repo + " && ./scripts/install-groka.sh --skip-build";
}

let objective = "build-install groka";
if skip_build {
    objective += " skip_build";
}

phase("TelemetryStart");
let tele_bin = "/Users/karan/.grok/bin/grok-workflow-telemetry";
let b0 = budget();
agent(
    "Run exactly: " + tele_bin + " snap --phase before --workflow groka "
        + "--objective " + json_encode(objective) + " "
        + "--budget-json " + json_encode(json_encode(b0)) + " "
        + "--source groka.rhai; return {ok, detail}.",
    #{ label: "telemetry:before", capability_mode: "execute", output_schema: tele_schema },
);

phase("BuildInstall");
let prompt = "";
prompt += "Build and install the groka fork binary so its --version stamp matches git HEAD.\n\n";
prompt += "Working directory MUST be: " + repo + "\n\n";
prompt += "Run this exact command (release build can take 10–20+ minutes; use a long or 0 timeout):\n\n";
prompt += "  " + cmd + "\n\n";
prompt += "Why: crates/codegen/xai-grok-pager-bin/build.rs bakes `git rev-parse --short HEAD` at compile time.\n";
prompt += "install-groka.sh touches that build.rs, cargo builds -p xai-grok-pager-bin --release, installs to\n";
prompt += "~/.local/bin/groka, and FAILS if `groka --version` does not contain the current short HEAD.\n\n";
prompt += "Rules:\n";
prompt += "- Do NOT edit application source for a normal rebuild.\n";
prompt += "- Do NOT use GitHub Actions; this is local only.\n";
prompt += "- After the script exits 0, also run: git -C " + repo + " rev-parse --short HEAD\n";
prompt += "  and: ~/.local/bin/groka --version\n";
prompt += "- ok=true only if install-groka.sh exit code is 0 AND version_line contains head.\n";
prompt += "- Return {ok, summary, command, log_tail, head, version_line, binary_path}.\n";
prompt += "- binary_path should be ~/.local/bin/groka (expanded path OK).\n";
prompt += "- log_tail: last ~40 lines of build/install output.\n";

let r = agent(prompt, #{
    label: "install-groka",
    capability_mode: "execute",
    output_schema: result_schema,
});

phase("TelemetryEnd");
let b1 = budget();
let status = "failed";
if r != () && r.success && r.output != () && r.output.ok == true {
    status = "complete";
}
agent(
    "Run exactly: " + tele_bin + " snap --phase after --workflow groka "
        + "--budget-json " + json_encode(json_encode(b1)) + " "
        + "--status " + status + " --source groka.rhai; return {ok, detail}.",
    #{ label: "telemetry:after", capability_mode: "execute", output_schema: tele_schema },
);

if r == () || !r.success || r.output == () {
    complete(#{
        summary: "groka agent failed or returned nothing",
        ok: false,
    });
}
if r.output.ok != true {
    complete(#{
        summary: r.output.summary,
        ok: false,
        head: r.output.head,
        version_line: r.output.version_line,
        log_tail: r.output.log_tail,
    });
}
complete(#{
    summary: r.output.summary,
    ok: true,
    head: r.output.head,
    version_line: r.output.version_line,
    binary_path: r.output.binary_path,
    command: r.output.command,
});
