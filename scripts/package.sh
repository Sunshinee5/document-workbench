#!/usr/bin/env bash
# 绿色版组装脚本：把 release 产物 + 前端产物装配成 dist/DocumentWorkbench/。
#
# 设计原则：
#   1. 只覆盖「程序本体」（exe / dll / ui），**不动** config、lang、workspace、logs —— 这些是运行期数据；
#   2. 前端产物整目录替换，避免旧 hash 文件残留；
#   3. 缺哪个目录就补哪个，不删除已有内容。
#
# 用法：
#   bash scripts/package.sh            # 全量：编译 Rust + 构建前端 + 组装
#   bash scripts/package.sh --skip-rust # 只重建前端并组装
#   bash scripts/package.sh --skip-ui   # 只重编 Rust 并组装
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/dist/DocumentWorkbench"
SKIP_RUST=0
SKIP_UI=0

for arg in "$@"; do
  case "$arg" in
    --skip-rust) SKIP_RUST=1 ;;
    --skip-ui)   SKIP_UI=1 ;;
    *) echo "未知参数：$arg" >&2; exit 2 ;;
  esac
done

# ---------- 1. 前端 ----------
# 必须在前：前端产物是在 Rust 宏展开时被编进二进制的，顺序反了就会把上一轮的前端打进包里。
if [ "$SKIP_UI" -eq 0 ]; then
  echo "[1/4] 构建前端…"
  ( cd "$ROOT/ui" && npm run build )
else
  echo "[1/4] 跳过前端构建"
fi

# ---------- 2. Rust ----------
if [ "$SKIP_RUST" -eq 0 ]; then
  echo "[2/4] 编译 Rust（release）…"
  # 本机无 MSVC link.exe，必须显式指定 gnu 工具链，见 05 §7。
  # custom-protocol 必须开：tauri 的 build.rs 以 `!custom-protocol` 判定 dev，
  # 不带这个 feature 的二进制会去连 devUrl（localhost:5173），绿色版一打开就是「拒绝连接」。
  #
  # 必须先 touch lib.rs：前端产物是在宏展开时编进二进制的，而 cargo 的增量判定
  # 只看 Rust 源文件，前端换了 hash 它可能认为 dw-desktop 是 fresh 的，
  # 结果打出来的 exe 里还是旧前端、甚至一个前端都没有（表现为「asset not found: index.html」）。
  touch "$ROOT/apps/dw-desktop/src/lib.rs"
  ( cd "$ROOT" && cargo +stable-x86_64-pc-windows-gnu build -p dw-desktop --release --features custom-protocol )
else
  echo "[2/4] 跳过 Rust 编译"
fi

# ---------- 3. 校验产物 ----------
EXE="$ROOT/target/release/dw-desktop.exe"
DLL="$ROOT/target/release/WebView2Loader.dll"
UI="$ROOT/ui/dist"
for path in "$EXE" "$DLL" "$UI/index.html" "$UI/pdfjs/cmaps" "$UI/pdfjs/standard_fonts"; do
  if [ ! -e "$path" ]; then
    echo "缺少产物：$path" >&2
    exit 1
  fi
done

# 自检：确认「当前这一版前端」真的被编进了 exe。
# 只查一个泛化关键词是不够的——上一版前端也含同样关键词。
# 这里直接取 ui/dist 里实际的入口文件名去比对，比中了才算数。
ENTRY_JS="$(cd "$UI/assets" && ls index-*.js | head -1)"
if [ -z "$ENTRY_JS" ] || ! grep -aq "$ENTRY_JS" "$EXE"; then
  echo "自检失败：exe 内没有当前前端（期望包含 $ENTRY_JS）" >&2
  echo "  可能原因：没带 custom-protocol，或 dw-desktop 没重新编译。" >&2
  exit 1
fi
echo "  自检通过：exe 已内嵌 $ENTRY_JS"

# ---------- 4. 组装 ----------
echo "[3/4] 组装到 $OUT …"
mkdir -p "$OUT"

# 程序本体：直接覆盖
cp -f "$EXE" "$OUT/DocumentWorkbench.exe"
cp -f "$DLL" "$OUT/WebView2Loader.dll"

# 前端已经由 custom-protocol 编进 exe，不再单独放 ui/ 目录：
# 留着只会让人误改 dist 里的副本。旧版本留下的目录一并清掉。
rm -rf "$OUT/ui"

# 运行期目录：只补不删
for dir in \
  "config" \
  "lang" \
  "logs" \
  "cache" \
  "plugins" \
  "resources/fonts" \
  "runtime/cache" \
  "Recovery/temp" \
  "workspace/cache" \
  "workspace/current" \
  "workspace/recent" \
  "workspace/temp"; do
  mkdir -p "$OUT/$dir"
done

# 配置文件只在缺失时写入默认值，不覆盖用户改动
if [ ! -f "$OUT/config/app.json" ]; then
  cat > "$OUT/config/app.json" <<'JSON'
{
  "title": "文档工作台",
  "language": "zh_CN"
}
JSON
fi

echo "[4/4] 完成"
echo "  exe : $(du -h "$OUT/DocumentWorkbench.exe" | cut -f1)（前端与 pdfjs 资源已内嵌）"
echo "  总计: $(du -sh "$OUT" | cut -f1)"
