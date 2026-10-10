#!/usr/bin/env python3
"""Charts and an HTML report for the results of the profiling suite (tests/profile).

Reads the JSON files in a results directory (default: target/profile in the checkout) and writes:
  charts/latency.png    input latency, cumulative distribution, idle and under a 1 kHz flood
  charts/startup.png    daemon start-up, cold and warm, to socket and to the pad being grabbed
  charts/gui.png        settings-window start-up, one dot per run
  charts/resources.png  daemon CPU and memory, idle and under flood; settings window while up
  report.html           the same charts, each with its numbers as a table

A topic whose JSON is missing is skipped, so the script works on a partial run.

Usage: scripts/plot_profile.py [results directory]
Needs matplotlib.
"""
import html
import json
import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

# Light-mode values from the validated default palette (dataviz skill, references/palette.md):
# categorical slots 1 (blue) and 2 (orange), which pass the CVD and contrast checks for two series.
BLUE = "#2a78d6"
ORANGE = "#eb6834"
INK = "#0b0b0b"
INK_SECONDARY = "#52514e"
SURFACE = "#fcfcfb"
GRID = "#e4e3df"


def style(ax):
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    for side in ("left", "bottom"):
        ax.spines[side].set_color(INK_SECONDARY)
    ax.tick_params(colors=INK_SECONDARY)
    ax.yaxis.label.set_color(INK_SECONDARY)
    ax.xaxis.label.set_color(INK_SECONDARY)
    ax.grid(axis="y", color=GRID, linewidth=0.8)
    ax.set_axisbelow(True)


def figure(width=8.0, height=4.5, panels=1):
    fig, axes = plt.subplots(1, panels, figsize=(width, height), dpi=150, facecolor=SURFACE)
    axes = [axes] if panels == 1 else list(axes)
    for ax in axes:
        ax.set_facecolor(SURFACE)
        style(ax)
    return fig, axes


def load(results, name):
    path = results / f"{name}.json"
    if not path.exists():
        return None
    return json.loads(path.read_text())


def ms(value):
    return f"{value:.1f} ms"


def latency_chart(data, out):
    if not any((data.get(k) or {}).get("samples_us") for k in ("idle", "flood_1khz")):
        return False
    fig, (ax,) = figure()
    handles = []
    for key, label, color in [("idle", "Idle", BLUE), ("flood_1khz", "1 kHz stick flood", ORANGE)]:
        condition = data.get(key)
        if not condition or not condition.get("samples_us"):
            continue
        samples = sorted(condition["samples_us"])
        share = [(i + 1) / len(samples) for i in range(len(samples))]
        stats = condition["latency"]
        ax.step(samples, share, where="post", color=color, linewidth=2)
        handles.append(plt.Line2D([], [], color=color, linewidth=2,
                                  label=f"{label}: p50 {stats['p50_us']:.0f} µs, p99 {stats['p99_us']:.0f} µs"))
    ax.set_xscale("log")
    ax.set_xlabel("press to output, µs (log scale)")
    ax.set_ylabel("share of presses at or below")
    ax.set_ylim(0, 1.0)
    ax.set_yticks([0, 0.5, 0.9, 1.0])
    ax.set_yticklabels(["0%", "50%", "90%", "100%"])
    ax.set_title("Input latency: South press to the daemon's virtual pad", color=INK, loc="left", fontsize=12)
    ax.grid(axis="x", color=GRID, linewidth=0.8)
    ax.legend(handles=handles, frameon=False, loc="upper left")
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)
    return True


def latency_table(data):
    rows = [["Condition", "Samples", "Missed", "p50 µs", "p90 µs", "p99 µs", "max µs", "Reports/s", "Output events/s"]]
    for key, label in [("idle", "Idle"), ("flood_1khz", "1 kHz stick flood")]:
        c = data.get(key)
        if not c:
            continue
        s = c["latency"]
        rows.append([
            label, s["count"], c["missed"], f"{s['p50_us']:.0f}", f"{s['p90_us']:.0f}", f"{s['p99_us']:.0f}",
            f"{s['max_us']:.0f}", f"{c['reports_per_second']:.0f}", f"{c['output_events_per_second']:.0f}",
        ])
    return rows


def load_history(results):
    path = results / "history.jsonl"
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def trend_chart(history, out):
    fig, (ax,) = figure(width=8.0, height=4.0)
    x = list(range(1, len(history) + 1))
    for key, label, color in [("idle", "Idle", BLUE), ("flood", "1 kHz stick flood", ORANGE)]:
        p99 = [run[f"{key}_p99_us"] for run in history]
        p50 = [run[f"{key}_p50_us"] for run in history]
        ax.plot(x, p99, color=color, marker="o", linewidth=2, label=f"{label}: p99")
        ax.plot(x, p50, color=color, marker="o", linewidth=1, linestyle="--", alpha=0.8, label=f"{label}: p50")
    ax.set_xticks(x)
    ax.set_xlabel("suite run (oldest on the left)")
    ax.set_ylabel("press to output, µs")
    ax.set_ylim(bottom=0)
    ax.set_title("Input latency across suite runs (solid p99, dashed p50)", color=INK, loc="left", fontsize=12)
    ax.legend(frameon=False, loc="upper left", ncol=2)
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)


def trend_table(history):
    rows = [["Run", "Commit", "Sessions", "Idle p50 µs", "Idle p99 µs", "Flood p50 µs", "Flood p99 µs"]]
    for i, run in enumerate(history, 1):
        rows.append([i, run.get("commit") or "-", run.get("sessions", "-"),
                     f"{run['idle_p50_us']:.0f}", f"{run['idle_p99_us']:.0f}",
                     f"{run['flood_p50_us']:.0f}", f"{run['flood_p99_us']:.0f}"])
    return rows


def sessions_table(data):
    rows = [["Session", "Idle p50 µs", "Idle p99 µs", "Idle missed", "Flood p50 µs", "Flood p99 µs", "Flood missed"]]
    for session in data.get("per_session", []):
        i, f = session["idle"], session["flood_1khz"]
        rows.append([session["repeat"] + 1, f"{i['p50_us']:.0f}", f"{i['p99_us']:.0f}", i["missed"],
                     f"{f['p50_us']:.0f}", f"{f['p99_us']:.0f}", f["missed"]])
    return rows


def overlay_chart(data, out):
    window = data.get("window") if isinstance(data.get("window"), dict) else None
    panels = 3 if window else 1
    fig, axes = figure(width=10.0 if window else 7.0, height=4.2, panels=panels)
    steps = [("open", "Open"), ("navigate", "Move cursor"), ("close", "Close")]
    response = data["daemon_response"]
    ax = axes[0]
    x = list(range(len(steps)))
    # A range per step, from p50 to p99, on a log scale: one slow close would flatten linear bars.
    for i, (key, _) in enumerate(steps):
        p50, p99 = response[key]["p50_us"], response[key]["p99_us"]
        ax.plot([i, i], [p50, p99], color=INK_SECONDARY, linewidth=2, zorder=2)
        ax.scatter([i], [p50], color=BLUE, s=48, zorder=3)
        ax.scatter([i], [p99], color=ORANGE, s=48, zorder=3)
        ax.annotate(f"p50 {p50:.0f}", xy=(i, p50), xytext=(-8, 0), textcoords="offset points", va="center", ha="right", color=INK, fontsize=9)
        ax.annotate(f"p99 {p99:.0f}", xy=(i, p99), xytext=(8, 0), textcoords="offset points", va="center", color=INK, fontsize=9)
    ax.set_yscale("log")
    ax.set_xlim(-0.5, len(steps) - 0.5)
    ax.set_xticks(x)
    ax.set_xticklabels([label for _, label in steps], color=INK)
    ax.set_ylabel("request to overlay frame, µs (log scale)")
    ax.set_title("Daemon response", color=INK, loc="left", fontsize=12)
    ax.legend(
        handles=[plt.Line2D([], [], marker="o", linestyle="", color=c, label=l) for c, l in [(BLUE, "p50"), (ORANGE, "p99")]],
        frameon=False, loc="upper left",
    )
    if window:
        draw = window["draw"]
        ax = axes[1]
        series = [("received_to_built", "frame to built"), ("build", "build alone")]
        labels, p50s, p99s = [], [], []
        for key, label in series:
            stats = draw.get(key, {})
            if stats.get("count"):
                labels.append(label)
                p50s.append(stats["p50_us"])
                p99s.append(stats["p99_us"])
        xs = list(range(len(labels)))
        for offset, values, name, color in [(-0.2, p50s, "p50", BLUE), (0.2, p99s, "p99", ORANGE)]:
            bars = ax.bar([v + offset for v in xs], values, width=0.38, color=color, label=name)
            for bar, value in zip(bars, values):
                ax.annotate(f"{value:.0f}", xy=(bar.get_x() + bar.get_width() / 2, value), xytext=(0, 3),
                            textcoords="offset points", ha="center", color=INK, fontsize=9)
        ax.set_xticks(xs)
        ax.set_xticklabels(labels, color=INK)
        ax.set_ylabel("µs")
        ax.set_title("Overlay draw", color=INK, loc="left", fontsize=12)
        ax.set_ylim(bottom=0)
        ax.legend(frameon=False, loc="upper right")
        ax = axes[2]
        states = [("hidden", "Hidden"), ("shown_while_moving", "Shown, cursor moving")]
        xs = list(range(len(states)))
        cpu = [window[k]["cpu_percent"] for k, _ in states]
        rss = [window[k]["rss_end_kb"] / 1024 for k, _ in states]
        bars = ax.bar(xs, cpu, width=0.5, color=[BLUE, ORANGE])
        for bar, value, mb in zip(bars, cpu, rss):
            ax.annotate(f"{value:.1f}% CPU, {mb:.0f} MB", xy=(bar.get_x() + bar.get_width() / 2, value),
                        xytext=(0, 3), textcoords="offset points", ha="center", color=INK, fontsize=9)
        ax.set_xticks(xs)
        ax.set_xticklabels([label for _, label in states], color=INK)
        ax.set_ylabel("overlay process CPU, % of one core")
        ax.set_title("Overlay process", color=INK, loc="left", fontsize=12)
        ax.set_ylim(0, max(cpu) * 1.35 + 0.5)
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)


def overlay_tables(data):
    response = data["daemon_response"]
    tables = [[["Step", "Count", "p50 µs", "p90 µs", "p99 µs", "max µs", "Missed"]] + [
        [label, response[k].get("count", 0), f"{response[k].get('p50_us', 0):.0f}", f"{response[k].get('p90_us', 0):.0f}",
         f"{response[k].get('p99_us', 0):.0f}", f"{response[k].get('max_us', 0):.0f}", response["missed"] if k == "open" else "-"]
        for k, label in [("open", "Open"), ("navigate", "Move cursor"), ("close", "Close")]
    ]]
    window = data.get("window")
    if isinstance(window, dict):
        draw = window["draw"]
        rows = [["Measure", "Count", "p50 µs", "p99 µs", "max µs"]]
        for key, label in [("received_to_built", "Frame received to widgets built"), ("build", "Widget build alone")]:
            st = draw.get(key, {})
            rows.append([label, st.get("count", 0), f"{st.get('p50_us', 0):.0f}", f"{st.get('p99_us', 0):.0f}", f"{st.get('max_us', 0):.0f}"])
        tables.append(rows)
        rows = [["State", "CPU %", "RSS MB", "Threads"]]
        for key, label in [("hidden", "Hidden"), ("shown_while_moving", "Shown, cursor moving")]:
            u = window[key]
            rows.append([label, f"{u['cpu_percent']:.2f}", f"{u['rss_end_kb'] / 1024:.1f}", u["threads_peak"]])
        tables.append(rows)
    return tables


def startup_chart(data, out):
    fig, (ax,) = figure()
    # Each measure gets its own slot: cold at 0 and 1, warm at 3 and 4, so no two series share an x.
    slots = {"cold": (0, 1), "warm": (3, 4)}
    measures = [("socket", "answers on its socket", BLUE), ("pad_managed", "grabs the pad", ORANGE)]
    for phase, (socket_x, pad_x) in slots.items():
        block = data.get(phase)
        if not block:
            continue
        for (key, _, color), x in zip(measures, (socket_x, pad_x)):
            values = block[key]["all_ms"]
            ax.scatter([x] * len(values), values, color=color, s=38, zorder=3)
            if values:
                median = block[key]["median_ms"]
                ax.plot([x - 0.2, x + 0.2], [median, median], color=color, linewidth=2.5, zorder=4)
                ax.annotate(f"{median:.0f} ms", xy=(x, median), xytext=(0, 8), textcoords="offset points",
                            ha="center", va="bottom", color=INK, fontsize=10)
    ax.set_xticks([0.5, 3.5])
    ax.set_xticklabels(["Cold: first launch", "Warm: later launch"], color=INK)
    ax.set_xticks([0, 1, 3, 4], minor=True)
    ax.set_xticklabels(["socket", "pad", "socket", "pad"], minor=True, color=INK_SECONDARY, fontsize=9)
    ax.tick_params(axis="x", which="minor", length=0, pad=2)
    ax.tick_params(axis="x", which="major", length=0, pad=18)
    ax.set_xlim(-0.6, 4.6)
    ax.set_ylim(bottom=0)
    ax.set_ylabel("time from launch, ms")
    ax.set_title("Daemon start-up (dots are runs, bars are medians)", color=INK, loc="left", fontsize=12)
    ax.legend(
        handles=[plt.Line2D([], [], marker="o", linestyle="", color=c, label=l) for _, l, c in measures],
        frameon=False,
        loc="upper center",
        bbox_to_anchor=(0.5, -0.14),
        ncol=2,
    )
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)


def startup_table(data):
    rows = [["Start", "Measure", "Runs", "Median ms", "Min ms", "Max ms"]]
    for phase in ["cold", "warm"]:
        block = data.get(phase)
        if not block:
            continue
        for key, label in [("socket", "answers on socket"), ("pad_managed", "grabs the pad")]:
            s = block[key]
            rows.append([phase, label, s["runs"], f"{s['median_ms']:.0f}", f"{s['min_ms']:.0f}", f"{s['max_ms']:.0f}"])
    return rows


def gui_chart(data, out):
    fig, (ax,) = figure(width=7.0, height=4.0)
    shown = [run["window_ms"] for run in data["runs"] if run.get("window_ms") is not None]
    ax.scatter([0] * len(shown), shown, color=BLUE, s=42, zorder=3)
    if shown:
        median = data["window_ms"]["median_ms"]
        ax.plot([-0.12, 0.12], [median, median], color=BLUE, linewidth=2.5, zorder=4)
        ax.annotate(f"median {median:.0f} ms", xy=(0.14, median), va="center", color=INK, fontsize=10)
    ax.set_xticks([0])
    ax.set_xticklabels(["padwight gui, daemon already running"], color=INK)
    ax.set_xlim(-0.5, 0.9)
    ax.set_ylim(bottom=0)
    ax.set_ylabel("time from launch to window, ms")
    ax.set_title("Settings window start-up (X11 path)", color=INK, loc="left", fontsize=12)
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)


def gui_table(data):
    rows = [["Run", "Window ms", "CPU % while up", "RSS start MB", "RSS end MB", "Threads"]]
    for i, run in enumerate(data["runs"], 1):
        u = run.get("usage_while_up") or {}
        rows.append([
            i,
            "-" if run.get("window_ms") is None else f"{run['window_ms']:.0f}",
            f"{u.get('cpu_percent', 0):.1f}",
            f"{u.get('rss_start_kb', 0) / 1024:.0f}",
            f"{u.get('rss_end_kb', 0) / 1024:.0f}",
            u.get("threads_peak", "-"),
        ])
    return rows


def resources_chart(data, out):
    fig, axes = figure(width=10.0, height=4.2, panels=2)
    idle = data.get("idle")
    flood = (data.get("flood_1khz") or {}).get("usage")
    if idle and flood:
        ax = axes[0]
        cpu = [idle["cpu_percent"], flood["cpu_percent"]]
        bars = ax.bar(["Idle", "1 kHz flood"], cpu, color=[BLUE, ORANGE], width=0.5)
        for bar, value in zip(bars, cpu):
            ax.annotate(f"{value:.1f}%", xy=(bar.get_x() + bar.get_width() / 2, value), xytext=(0, 4),
                        textcoords="offset points", ha="center", color=INK, fontsize=10)
        ax.set_ylabel("daemon CPU, % of one core")
        ax.set_title("Daemon CPU", color=INK, loc="left", fontsize=12)
        ax.set_ylim(0, max(cpu) * 1.25 + 0.5)
    ax = axes[1]
    if idle and flood:
        rss = [idle["rss_end_kb"] / 1024, flood["rss_end_kb"] / 1024]
        bars = ax.bar(["Idle", "After flood"], rss, color=[BLUE, ORANGE], width=0.5)
        for bar, value in zip(bars, rss):
            ax.annotate(f"{value:.1f} MB", xy=(bar.get_x() + bar.get_width() / 2, value), xytext=(0, 4),
                        textcoords="offset points", ha="center", color=INK, fontsize=10)
        ax.set_ylabel("daemon resident memory, MB")
        ax.set_title("Daemon memory", color=INK, loc="left", fontsize=12)
        ax.set_ylim(0, max(rss) * 1.25)
    fig.tight_layout()
    fig.savefig(out, facecolor=SURFACE)
    plt.close(fig)


def resources_table(data):
    rows = [["Phase", "CPU %", "RSS start MB", "RSS end MB", "Growth KB", "Threads", "fds", "Output events/s", "Dropped"]]
    idle = data.get("idle")
    flood = data.get("flood_1khz")
    if idle:
        rows.append(["Idle", f"{idle['cpu_percent']:.2f}", f"{idle['rss_start_kb'] / 1024:.1f}",
                     f"{idle['rss_end_kb'] / 1024:.1f}", idle["rss_growth_kb"], idle["threads_peak"], idle["fds_peak"], "-", "-"])
    if flood:
        u, io = flood["usage"], flood["io"]
        rows.append(["1 kHz flood", f"{u['cpu_percent']:.2f}", f"{u['rss_start_kb'] / 1024:.1f}",
                     f"{u['rss_end_kb'] / 1024:.1f}", u["rss_growth_kb"], u["threads_peak"], u["fds_peak"],
                     f"{io['output_events_per_second']:.0f}", io["dropped_output_events"]])
    return rows


def html_table(rows):
    head, *body = rows
    th = "".join(f"<th>{html.escape(str(c))}</th>" for c in head)
    trs = "".join("<tr>" + "".join(f"<td>{html.escape(str(c))}</td>" for c in row) + "</tr>" for row in body)
    return f"<table><thead><tr>{th}</tr></thead><tbody>{trs}</tbody></table>"


def build_report(results, sections, environment):
    env_rows = "".join(f"<tr><th>{html.escape(k)}</th><td>{html.escape(str(v))}</td></tr>" for k, v in environment.items())
    body = []
    for title, chart, caption, tables in sections:
        body.append(
            f"<section><h2>{html.escape(title)}</h2>"
            f"<img src=\"charts/{chart}\" alt=\"{html.escape(title)}\">"
            f"<p class=\"caption\">{html.escape(caption)}</p>"
            + "".join(html_table(t) for t in tables)
            + "</section>"
        )
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Padwight profile</title>
<style>
  body {{ margin: 0; padding: 16px; font: 15px/1.5 system-ui, sans-serif; background: {SURFACE}; color: {INK}; }}
  main {{ max-width: 960px; margin: 0 auto; }}
  h1 {{ font-size: 22px; margin: 8px 0 16px; }}
  h2 {{ font-size: 17px; margin: 28px 0 8px; }}
  img {{ max-width: 100%; height: auto; }}
  .caption {{ color: {INK_SECONDARY}; font-size: 13px; margin: 6px 0 10px; }}
  table {{ border-collapse: collapse; font-size: 13px; width: 100%; margin-bottom: 8px; }}
  th, td {{ text-align: left; padding: 4px 8px; border-bottom: 1px solid {GRID}; }}
  thead th {{ color: {INK_SECONDARY}; font-weight: 600; }}
  td {{ font-variant-numeric: tabular-nums; }}
  .env th {{ width: 180px; color: {INK_SECONDARY}; font-weight: 400; }}
</style></head>
<body><main>
<h1>Padwight profile</h1>
<table class="env"><tbody>{env_rows}</tbody></table>
{''.join(body)}
</main></body></html>
"""


def main():
    results = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "target" / "profile"
    charts = results / "charts"
    charts.mkdir(parents=True, exist_ok=True)
    sections = []
    environment = {}

    latency = load(results, "input_latency")
    if latency:
        environment = latency.get("environment", environment)
    if latency and latency_chart(latency, charts / "latency.png"):
        sections.append((
            "Input latency",
            "latency.png",
            "Each line is the share of presses that reached the daemon's virtual pad within a given time. "
            "The line further left is faster. Measured from the write to the uhid pad to the kernel time stamp of BTN_SOUTH.",
            [latency_table(latency), sessions_table(latency)],
        ))
        history = load_history(results)
        if len(history) >= 2:
            trend_chart(history, charts / "latency_trend.png")
            sections.append((
                "Latency across runs",
                "latency_trend.png",
                "Each suite run appends its pooled p50 and p99 to history.jsonl. A rise in p99 across runs "
                "is the thing to watch.",
                [trend_table(history)],
            ))

    startup = load(results, "daemon_startup")
    if startup:
        environment = environment or startup.get("environment", {})
        startup_chart(startup, charts / "startup.png")
        sections.append((
            "Daemon start-up",
            "startup.png",
            "Cold: no config, binary and libraries evicted from the page cache. Warm: config present, binary cached. "
            "Times run from launch.",
            [startup_table(startup),],
        ))

    gui = load(results, "gui_startup")
    if gui:
        environment = environment or gui.get("environment", {})
        gui_chart(gui, charts / "gui.png")
        sections.append((
            "Settings window start-up",
            "gui.png",
            "From `padwight gui` to the window being mapped in the X server, with the daemon already running. "
            "Mapped is not first paint.",
            [gui_table(gui),],
        ))

    overlay = load(results, "overlay")
    if overlay:
        environment = environment or overlay.get("environment", {})
        overlay_chart(overlay, charts / "overlay.png")
        caption = "Daemon response: request or stick push to the next overlay frame. "
        if isinstance(overlay.get("window"), dict):
            caption += "Window: a Wayland session, so draw and process figures are included."
        else:
            caption += "Window figures were skipped: " + str(overlay.get("window", "not run")) + "."
        sections.append(("Keyboard overlay", "overlay.png", caption, overlay_tables(overlay)))

    resources = load(results, "resources")
    if resources:
        environment = environment or resources.get("environment", {})
        resources_chart(resources, charts / "resources.png")
        sections.append((
            "Daemon CPU and memory",
            "resources.png",
            "Idle for 10 s, then under a 1 kHz stick flood for 10 s, with a controller attached.",
            [resources_table(resources),],
        ))

    if not sections:
        sys.exit(f"no results in {results}")
    (results / "report.html").write_text(build_report(results, sections, environment))
    for _, chart, _, _ in sections:
        print(charts / chart)
    print(results / "report.html")


if __name__ == "__main__":
    main()
