//保留 alloc 中已有的集合
pub use alloc::collections::*;

//HashMap 的具体实现放在子模块中
mod hashmap;

//通过 collections::HashMap 对外提供这个类型
pub use self::hashmap::HashMap;

