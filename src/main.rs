use kernel_core::{Kernel, KernelConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = KernelConfig::default();
    config.base_dir = std::env::current_dir()?;

    let kernel = Kernel::new(config)?;

    println!("running python worker");
    let python_report = kernel.run_python("python_app/main.py")?;
    println!("python report: {:?}", python_report);

    println!("running lua worker");
    let lua_report = kernel.run_lua("lua_app/main.lua")?;
    println!("lua report: {:?}", lua_report);

    println!("running inline python");
    let inline_report = kernel.run_python(r#"
print('inline python execution')
"#)?;
    println!("inline report: {:?}", inline_report);

    println!("running inline lua");
    let inline_lua = kernel.run_lua(r#"
print('inline lua execution')
"#)?;
    println!("inline lua report: {:?}", inline_lua);

    Ok(())
}
