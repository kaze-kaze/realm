"""Render result json files as markdown tables. Usage: mkmd.py out_dir name [name..]"""
import json, sys
d = sys.argv[1]
ICON = {"PASS": "✅ 通过", "FAIL": "❌ 失败", "INFO": "ℹ️ 说明"}
for name in sys.argv[2:]:
    rs = json.load(open(f"{d}/{name}.json"))
    print(f"\n<!-- {name} -->\n| 编号 | 场景 | 预期 | 实测 | 结果 |\n| --- | --- | --- | --- | --- |")
    for r in rs:
        obs = r["observed"].replace("|", "\\|").replace("\n", " ")
        print(f"| {r['id']} | {r['title']} | {r['expect']} | {obs} | {ICON.get(r['status'], r['status'])} |")
    n = {k: sum(1 for r in rs if r["status"] == k) for k in ICON}
    print(f"\n<!-- {name}: {n} -->")
