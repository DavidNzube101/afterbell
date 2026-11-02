BUILD_DIR ?= backend/build
CMAKE_BUILD_TYPE ?= Debug
CMAKE ?= cmake
CLANG_TIDY ?= clang-tidy
CLANG_TIDY_CHECKS ?= clang-analyzer-*,bugprone-*,performance-*
CMAKE_PREFIX_PATH ?=

SOURCES := $(shell find backend/src -type f -name '*.cpp' | sort)
CMAKE_PREFIX_ARG = $(if $(strip $(CMAKE_PREFIX_PATH)),-DCMAKE_PREFIX_PATH="$(subst :,;,$(CMAKE_PREFIX_PATH))",)

.PHONY: configure lint build api indexer dev

configure:
	$(CMAKE) -S backend -B $(BUILD_DIR) \
		-DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE) \
		-DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
		$(CMAKE_PREFIX_ARG)

lint: configure
	@if ! command -v "$(CLANG_TIDY)" >/dev/null 2>&1; then \
		echo "clang-tidy is required. On macOS: brew install llvm, then add its bin directory to PATH."; \
		exit 1; \
	fi
	$(CLANG_TIDY) --checks="$(CLANG_TIDY_CHECKS)" -p $(BUILD_DIR) $(SOURCES)

build: configure
	$(CMAKE) --build $(BUILD_DIR) --parallel

api: build
	cd backend && exec "$(abspath $(BUILD_DIR))/afterbell-api"

indexer: build
	cd backend && exec "$(abspath $(BUILD_DIR))/afterbell-indexer"

dev: indexer
