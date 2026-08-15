TARGET := stampede

CC := cc
CFLAGS := -std=c11 -Wall -Wextra -Wpedantic -DGL_SILENCE_DEPRECATION
LDFLAGS :=

BUILD_DIR := build
SRC := $(wildcard src/*.c)
OBJ := $(patsubst src/%.c,$(BUILD_DIR)/%.o,$(SRC))
BIN := $(BUILD_DIR)/$(TARGET)

.PHONY: all clean run

all: $(BIN)

$(BIN): $(OBJ)
	$(CC) $(OBJ) -o $@ $(LDFLAGS)

$(BUILD_DIR)/%.o: src/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

run: $(BIN)
	./$(BIN) $(ARGS)

clean:
	rm -rf $(BUILD_DIR)
