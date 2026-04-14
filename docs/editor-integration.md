# Editor Integration

`modum` is workspace-oriented, so editor integration works best when the editor knows which workspace root to analyze and treats diagnostics as non-fatal.

General guidance:

- use `--mode warn` so editor runs don't fail the job
- use `--format json` for stable parsing
- resolve the workspace root explicitly if one session spans several crates
- prefer run-on-save over running on every `InsertLeave`

## Neovim With `nvim-lint`

```lua
local lint = require("lint")

lint.linters.modum = {
  cmd = "modum",
  stdin = false,
  stream = "stdout",
  args = { "check", "--root", vim.fn.getcwd(), "--mode", "warn", "--format", "json" },
  parser = function(output, bufnr)
    if output == "" then
      return {}
    end

    local decoded = vim.json.decode(output)
    local current_file = vim.api.nvim_buf_get_name(bufnr)
    local diagnostics = {}

    for _, item in ipairs(((decoded or {}).report or {}).diagnostics or {}) do
      if item.file == current_file then
        diagnostics[#diagnostics + 1] = {
          bufnr = bufnr,
          lnum = math.max((item.line or 1) - 1, 0),
          col = 0,
          severity = item.level == "Error"
            and vim.diagnostic.severity.ERROR
            or vim.diagnostic.severity.WARN,
          source = "modum",
          code = item.code,
          message = item.message,
        }
      end
    end

    return diagnostics
  end,
}

lint.linters_by_ft.rust = { "modum" }
```

If you edit multiple crates from one Neovim session, replace `vim.fn.getcwd()` with your workspace root resolver.
