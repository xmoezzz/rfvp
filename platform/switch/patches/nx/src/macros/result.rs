#![macro_use]

/// Creates a result definition
///
/// # Examples
///
/// ```
/// // Defines "ResultDemo" result definition
/// result_define!(Demo: 345, 6789);
///
/// // Creates a "ResultCode" of value "2345-6789"
/// let rc = ResultDemo::make();
/// ```
#[macro_export]
macro_rules! result_define {
    (
        $(#[$meta:meta])*
        $name:ident: $module:expr, $description:expr
    ) => {
        paste::paste! {
            #[allow(missing_docs)]
            $(#[$meta])*
            pub struct [<Result $name>];

            impl $crate::result::ResultBase for [<Result $name>] {
                fn get_module() -> u32 {
                    $module
                }

                fn get_description() -> u32 {
                    $description
                }
            }
        }
    };
}

/// Creates a group of result definitions (all under the same module)
///
/// # Examples
///
/// ```
/// // Defines "ResultDemo1" and "ResultDemo2"
/// result_define_group!(345 => {
///     Demo1: 12,
///     Demo2: 15
/// });
///
/// // Creates a "ResultCode" of value "2345-0012"
/// let rc1 = ResultDemo1::make();
/// // Creates a "ResultCode" of value "2345-0015"
/// let rc2 = ResultDemo2::make();
/// ```
#[macro_export]
macro_rules! result_define_group {
    ($module:expr => { $( $name:ident: $description:expr ),* }) => {
        $( $crate::result_define!($name: $module, $description); )*
    };
}

/// Creates a group of result definitions (all under the same module and submodule)
///
/// # Examples
///
/// ```
/// // Defines "ResultDemo1" and "ResultDemo2"
/// result_define_subgroup!(345, 6000 => {
///     Demo1: 12,
///     Demo2: 15
/// });
///
/// // Creates a "ResultCode" of value "2345-6012"
/// let rc1 = ResultDemo1::make();
/// // Creates a "ResultCode" of value "2345-6015"
/// let rc2 = ResultDemo2::make();
/// ```
#[macro_export]
macro_rules! result_define_subgroup {
    (
        $module:expr, $submodule:expr => {
            $( $name:ident: $description:expr ),*
        }
    ) => {
        $crate::result_define_group!($module => { $( $name: $submodule + $description ),* });
    };
}

/// Returns a given result if the given condition is true
///
/// # Examples
///
/// ```
/// fn demo() -> Result<()> {
///     let cond: bool = (...);
///
///     // Specifying result definition types
///     result_return_if!(cond, ResultTest);
///
///     // Giving raw values
///     result_return_if!(cond, 0xBABE);
///
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! result_return_if {
    ($cond_expr:expr, $res:ty) => {
        let cond = $cond_expr;
        if cond {
            return <$res>::make_err();
        }
    };

    ($cond_expr:expr, $res:literal) => {
        let cond = $cond_expr;
        if cond {
            return $crate::result::ResultCode::new_err($res);
        }
    };
}

/// Returns a given result unless the given condition is true
///
/// # Examples
///
/// ```
/// fn demo() -> Result<()> {
///     let cond: bool = (...);
///
///     // Specifying result definition types
///     result_return_unless!(cond, ResultTest);
///
///     // Giving raw values
///     result_return_unless!(cond, 0xBABE);
///
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! result_return_unless {
    ($cond_expr:expr, $res:ty) => {
        $crate::result_return_if!(!$cond_expr, $res);
    };

    ($cond_expr:expr, $res:literal) => {
        $crate::result_return_if!(!$cond_expr, $res);
    };
}

/// Wraps and returns a given result if it's not successful
///
/// # Examples
///
/// ```
/// fn demo() -> Result<()> {
///     // Won't do anything
///     result_try!(ResultCode::new(0));
///     result_try!(ResultSuccess::make());    
///
///     // Will exit with the given results
///     result_try!(ResultCode::new(0xCAFE));
///     result_try!(ResultDemo::make());
///
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! result_try {
    ($rc_expr:expr) => {
        let rc = $rc_expr;
        if rc.is_failure() {
            return Err(rc);
        }
    };
}
