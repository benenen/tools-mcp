# tools-mcp Makefile
#
# 常用 cargo 工作流的薄封装。所有目标都委托给 cargo，
# 不引入额外构建产物或副作用。

CARGO ?= cargo
BIN   := tools-mcp

.DEFAULT_GOAL := help
.PHONY: help build release run test check clippy fmt fmt-check doc clean install uninstall ci

help: ## 列出所有目标
	@awk 'BEGIN{FS=":.*##"} /^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## debug 构建
	$(CARGO) build

release: ## release 构建（target/release/$(BIN)）
	$(CARGO) build --release

run: ## 跑 debug binary，用 ARGS 传参（make run ARGS="mysql --help"）
	$(CARGO) run -- $(ARGS)

test: ## 跑全部 unit + integration 测试
	$(CARGO) test

check: ## 类型检查（不产物）
	$(CARGO) check --all-targets

clippy: ## clippy lint，把 warnings 当错误
	$(CARGO) clippy --all-targets -- -D warnings

fmt: ## rustfmt 直接改文件
	$(CARGO) fmt --all

fmt-check: ## CI 用，不改文件只检查
	$(CARGO) fmt --all -- --check

doc: ## 构建 rustdoc 并打开
	$(CARGO) doc --no-deps --open

clean: ## 删除 target/
	$(CARGO) clean

install: release ## 把 release binary 装到 ~/.cargo/bin
	$(CARGO) install --path . --force

uninstall: ## 从 ~/.cargo/bin 卸载
	$(CARGO) uninstall $(BIN)

ci: fmt-check clippy test ## CI 流水：格式 → lint → 测试
