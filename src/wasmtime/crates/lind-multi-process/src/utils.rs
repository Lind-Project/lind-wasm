use anyhow::{Ok, Result};

// parse the path from the caller's memory space
pub fn parse_path(path: u64) -> Result<String> {
    let path_str = typemap::get_cstr_lossy(path).map_err(anyhow::Error::msg)?;
    Ok(path_str)
}

// parse the argv from the caller's memory space
pub fn parse_argv(argv: u64) -> Result<Vec<String>> {
    let argv_ptr = argv as *const u64;
    let mut args = Vec::new();

    // convert the address into a list of argument string
    let mut index = 0;

    // parse the arg pointers
    // Iterate over argv until we encounter a NULL pointer
    loop {
        let arg_ptr = unsafe { *(argv_ptr.add(index)) };

        if arg_ptr == 0 {
            break; // Stop if we encounter NULL
        }

        let arg = typemap::get_cstr_lossy(arg_ptr).map_err(anyhow::Error::msg)?;
        args.push(arg);

        index += 1; // Move to the next argument
    }

    Ok(args)
}

// parse the environment variables from the caller's memory space
pub fn parse_env(env_ptr: u64) -> Result<Option<Vec<(String, Option<String>)>>> {
    let mut environs = None;

    let env_ptr = env_ptr as *const u64;
    let mut env_vec = Vec::new();

    let mut index = 0;

    // Iterate over argv until we encounter a NULL pointer
    loop {
        let env_ptr = unsafe { *(env_ptr.add(index)) };

        if env_ptr == 0 {
            break; // Stop if we encounter NULL
        }

        let env = typemap::get_cstr_lossy(env_ptr).map_err(anyhow::Error::msg)?;

        let parsed = parse_env_var(&env);
        env_vec.push(parsed);

        index += 1; // Move to the next argument
    }

    if env_vec.len() > 0 {
        environs = Some(env_vec);
    }

    Ok(environs)
}

// parse an environment variable, return its name and value
fn parse_env_var(env_var: &str) -> (String, Option<String>) {
    // Find the position of the first '=' character
    if let Some(pos) = env_var.find('=') {
        // If '=' is found, return the key and value as String and Some(String)
        let key = env_var[..pos].to_string();
        let value = env_var[pos + 1..].to_string();
        (key, Some(value))
    } else {
        // If '=' is not found, return the whole string as the key and None for the value
        (env_var.to_string(), None)
    }
}
