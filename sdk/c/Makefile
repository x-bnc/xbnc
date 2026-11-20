# Builds the example plugins. Needs clang and wasm-ld (LLVM 15 or newer),
# and llvm-objcopy (or `xbncd plugin pack`) to embed each manifest.
ifeq ($(origin CC),default)
CC = clang
endif
CFLAGS = --target=wasm32 -O2 -nostdlib -fno-builtin -Wall -Wextra -Wno-unused-function \
         -Wl,--no-entry -Wl,--export-dynamic -Wl,--strip-debug -Wl,-z,stack-size=65536

EXAMPLES = $(patsubst examples/%.c,examples/%.wasm,$(wildcard examples/*.c))

all: $(EXAMPLES)

OBJCOPY ?= llvm-objcopy

examples/%.wasm: examples/%.c examples/%.json xbnc_plugin.h xbnc_util.h
	$(CC) $(CFLAGS) -o $(@:.wasm=.raw.wasm) $<
	$(OBJCOPY) --add-section xbnc.manifest=$(<:.c=.json) $(@:.wasm=.raw.wasm) $@
	rm -f $(@:.wasm=.raw.wasm)

# The official plugins shipped inside xbncd (its plugin catalog).
OFFICIAL = seen remind tell poll chanstats antiflood dice autoresponder trivia greet urltitle factoids weather news karma quotes badwords games
bundle: $(EXAMPLES)
	mkdir -p ../../crates/xbncd/catalog
	for p in $(OFFICIAL); do cp examples/$$p.wasm ../../crates/xbncd/catalog/; done

clean:
	rm -f examples/*.wasm
