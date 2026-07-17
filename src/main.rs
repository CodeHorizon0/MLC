use kernel_core::{Kernel, KernelConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = KernelConfig::default();
    config.base_dir = std::env::current_dir()?;

    let kernel = Kernel::new(config)?;

    println!("\nPython:\n");
    let python_report = kernel.run_python("examples/python_app/main.py")?;
    println!("python report: {:?}", python_report);

    println!("\nLua:\n");
    let lua_report = kernel.run_lua("examples/lua_app/main.lua")?;
    println!("lua report: {:?}", lua_report);

    println!("\nJavaScript:\n");
    let js_report = kernel.run_js("examples/js_app/main.js")?;
    println!("JavaScript report: {:?}", js_report);


    Ok(())
}
