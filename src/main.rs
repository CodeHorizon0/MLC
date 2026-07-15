use kernel_core::{Kernel, KernelConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let kernel = Kernel::new(KernelConfig::default())?;

    let python_demo = r#"./main.py"#;

//     let lua_demo = r#"
// local host = host
// print('lua demo', host.cwd())
// "#;

    let report_py = kernel.run_python(python_demo)?;
    println!("python report: {:?}", report_py);

    // let report_lua = kernel.run_lua(lua_demo)?;
    // println!("lua report: {:?}", report_lua);

    Ok(())
}
