//! Built-in transports.

pub mod http;
pub mod local;

/// A generic struct for configuration of `Transport`.
#[derive(Clone)]
pub struct TransportConfig<L: crate::core::HList, InfoType, TargetInfoType> {
    /// The information about locations
    pub info: std::collections::HashMap<String, InfoType>,
    pub target_info: (String, TargetInfoType),
    /// The struct is parametrized by the location set (`L`).
    pub location_set: std::marker::PhantomData<L>,

}

/// This macro makes a `TransportConfig`.
// #[macro_export]
// macro_rules! transport_config {
//     ( $( $loc:ident : $val:expr ),* $(,)? ) => {
//         {
//             let mut config = std::collections::HashMap::new();
//             $(
//                 config.insert($loc::name().to_string(), $val);
//             )*

//             $crate::transport::TransportConfig::<$crate::LocationSet!($( $loc ),*), _> {
//                 info: config,
//                 location_set: core::marker::PhantomData
//             }
//         }
//     };
// }


/// This macro makes a `TransportConfig`; V2.
#[macro_export]
macro_rules! transport_config {
    ( $choreography_loc:ident, $( $loc:ident : $val:expr ),* $(,)? ) => {
        {
            let choreography_name = $choreography_loc::name().to_string();
            let mut config = std::collections::HashMap::new();
            let mut target_info = None;
            $(
                if $loc::name().to_string() != choreography_name{
                    config.insert($loc::name().to_string(), $val);
                } else {
                    println!("heyyyy");
                    target_info = Some($val);
                }
            )*

            // println!("{}, {}", target_info.unwrap().0, target.info.unwrap().1);

            $crate::transport::TransportConfig::<$crate::LocationSet!($( $loc ),*), _, _> {
                info: config,
                location_set: core::marker::PhantomData,
                target_info: (choreography_name, target_info.unwrap()),
            }
        }
    };
}