return {
    rust = {
        {
            type="codelldb",
            request="launch",
            name = "Debug",
            program="${workspaceFolder}/target/debug/gitui",
            args = {
            }
        }
    }
}
