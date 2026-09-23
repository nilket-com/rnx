# 0083 candidates and refusals, by decision

Rendered from `census.json` (gates 1 and 2). Every eligible, unavailable callable with function-level generics appears once, by inventory key and canonical path; `concrete` names the type arguments a release policy would choose.

## candidate: chained inference (22)

the item generic is bounded through the container generic; both resolve to owned script values

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:7611` | `DataFrame::drop_many` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_core:7555` | `DataFrame::explode` | `I`: IntoIterator<Item = S>; `S`: AsRef<str> | I = Vec<String>, S = String |  |
| `polars_core:7559` | `DataFrame::group_by` | `I`: IntoIterator<Item = S>; `S`: AsRef<str> | I = Vec<String>, S = String |  |
| `polars_core:7560` | `DataFrame::group_by_stable` | `I`: IntoIterator<Item = S>; `S`: AsRef<str> | I = Vec<String>, S = String |  |
| `polars_core:7676` | `DataFrame::partition_by` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_core:7677` | `DataFrame::partition_by_stable` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_core:7623` | `DataFrame::select` | `I`: IntoIterator<Item = S>; `S`: AsRef<str> | I = Vec<String>, S = String |  |
| `polars_lazy:307` | `LazyFrame::group_by` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_lazy:308` | `LazyFrame::group_by_stable` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_lazy:263` | `LazyFrame::rename` | `I`: IntoIterator<Item = T>; `J`: IntoIterator<Item = S>; `T`: AsRef<str>; `S`: AsRef<str> | I = Vec<String>, T = String, J = Vec<String>, S = String |  |
| `polars_plan:1540` | `Expr::over` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:1541` | `Expr::over_with_options` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:1555` | `Expr::sort_by` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:2882` | `as_list` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:2885` | `concat_expr` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:2883` | `concat_list` | `E`: AsRef<[IE]>; `IE`: Into<Expr> + Clone | E = Vec<Expr>, IE = Expr |  |
| `polars_plan:2911` | `by_name` | `S`: Into<PlSmallStr>; `I`: IntoIterator<Item = S> | I = Vec<String>, S = String |  |
| `polars_plan:2908` | `cols` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_plan:6911` | `StructNameSpace::drop` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_plan:6908` | `StructNameSpace::field_by_names` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_plan:6910` | `StructNameSpace::rename_fields` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> | I = Vec<String>, S = String |  |
| `polars_schema:276` | `Schema::try_project_with_alias` | `I`: IntoIterator<Item = (O, C)>; `O`: AsRef<str>; `C`: AsRef<str> | I = Vec<(String, String)>, O = String, C = String | generic owner |

## candidate: iterator input, owned items (3)

`Vec<Item>::into_iter()` satisfies Iterator, ExactSizeIterator and TrustedLen

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:2357` | `ListBooleanChunkedBuilder::append_iter` | `I`: Iterator<Item = Option<bool>> + TrustedLen | I = Vec<core::option::Option<bool>>::into_iter() |  |
| `polars_plan:7759` | `FileScanIR::gather_after_filter` | `I`: Iterator<Item = usize> | I = Vec<usize>::into_iter() |  |
| `polars_plan:6049` | `ScanSources::gather` | `impl Iterator<Item = usi`: Iterator<Item = usize> | impl Iterato = Vec<usize>::into_iter() |  |

## candidate: iterator input, owned items (per family) (1)

`Vec<Item>::into_iter()` on a generic owner: one binding per proven family pair

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:1317` | `ChunkSet::scatter_single` | `I`: IntoIterator<Item = IdxSize>; `Self`: Sized | I = Vec<polars_utils::index::IdxSize>::into_iter() | generic owner |

## candidate: iterator input, borrowed items (5)

a script vector of owned bytes or strings, borrowed for the call (`iter().map(as_slice|as_str)`)

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:2294` | `ListBinaryChunkedBuilder::append_trusted_len_iter` | `I`: Iterator<Item = Option<&[u8]>> + TrustedLen | I = Vec<owned core::option::Option<&[u8]>>::iter() |  |
| `polars_core:2295` | `ListBinaryChunkedBuilder::append_values_iter` | `I`: Iterator<Item = &[u8]> | I = Vec<owned &[u8]>::iter() |  |
| `polars_core:2232` | `ListStringChunkedBuilder::append_trusted_len_iter` | `I`: Iterator<Item = Option<&str>> + TrustedLen | I = Vec<owned core::option::Option<&str>>::iter() |  |
| `polars_core:2233` | `ListStringChunkedBuilder::append_values_iter` | `I`: Iterator<Item = &str> | I = Vec<owned &str>::iter() |  |
| `polars_dtype:371` | `FrozenCategories::new` | `I`: IntoIterator<Item = &str> | I = Vec<owned &str>::iter() |  |

## candidate: iterator input, borrowed items (per family) (1)

a script vector of owned bytes or strings, borrowed for the call (`iter().map(as_slice|as_str)`)

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3245` | `Logical::from_str_iter` | `I`: IntoIterator<Item = Option<&str>> | I = Vec<owned core::option::Option<&str>>::iter() | generic owner |

## candidate: family instantiation (20)

a function generic over a Polars type family (`T: PolarsNumericType`, `&ChunkedArray<T>`) needs the family applicability engine applied to function generics

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3669` | `ChunkedArray::retain_flags_from` | `U`: PolarsDataType |  | generic owner |
| `polars_core:3157` | `arg_max_cat` | `T`: PolarsCategoricalType |  |  |
| `polars_core:3155` | `arg_max_numeric` | `T`: PolarsNumericType |  |  |
| `polars_core:3156` | `arg_min_cat` | `T`: PolarsCategoricalType |  |  |
| `polars_core:3153` | `arg_min_numeric` | `T`: PolarsNumericType |  |  |
| `polars_core:2766` | `NewChunkedArray::from_iter_options` | `impl Iterator<Item = Opt`: Iterator<Item = Option<N>> |  | generic owner |
| `polars_core:2767` | `NewChunkedArray::from_iter_values` | `impl Iterator<Item = N>`: Iterator<Item = N> |  | generic owner |
| `polars_core:2484` | `ListPrimitiveChunkedBuilder::append_iter` | `I`: Iterator<Item = Option<T::Native>> + TrustedLen |  | generic owner |
| `polars_core:2483` | `ListPrimitiveChunkedBuilder::append_values_iter` | `I`: Iterator<Item = T::Native> |  | generic owner |
| `polars_core:2482` | `ListPrimitiveChunkedBuilder::append_values_iter_trusted_len` | `I`: Iterator<Item = T::Native> + TrustedLen |  | generic owner |
| `polars_core:2695` | `BinViewChunkedBuilder::append_option` | `S`: AsRef<T> |  | generic owner |
| `polars_core:2693` | `BinViewChunkedBuilder::append_value` | `S`: AsRef<T> |  | generic owner |
| `polars_core:832` | `float_arg_max_sorted_ascending` | `T`: PolarsNumericType |  |  |
| `polars_core:833` | `float_arg_max_sorted_descending` | `T`: PolarsNumericType |  |  |
| `polars_core:972` | `binary_search_ca` | `T`: PolarsDataType; `impl Iterator<Item = Opt`: Iterator<Item = Option<T::Physical>> |  |  |
| `polars_ops:77` | `peak_max_with_start_end` | `T`: PolarsNumericType |  |  |
| `polars_ops:81` | `peak_min_with_start_end` | `T`: PolarsNumericType |  |  |
| `polars_ops:1057` | `convert_and_bound_idx_ca` | `T`: PolarsIntegerType |  |  |
| `polars_ops:1062` | `new_int_range` | `T`: PolarsIntegerType |  |  |
| `polars_ops:1153` | `rle_lengths_helper_ca` | `T`: PolarsDataType |  |  |

## policy: numeric scalar (6)

`N: Num + NumCast` from a script number needs the release file to choose the concrete type (f64 or per family)

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3722` | `ChunkedArray::lhs_div` | `N`: num_traits::Num + num_traits::cast::NumCast |  | generic owner |
| `polars_core:3723` | `ChunkedArray::lhs_rem` | `N`: num_traits::Num + num_traits::cast::NumCast |  | generic owner |
| `polars_core:3719` | `ChunkedArray::lhs_sub` | `N`: num_traits::Num + num_traits::cast::NumCast |  | generic owner |
| `polars_core:4867` | `convert_time_units` | `T`: num_traits::Num + num_traits::cast::NumCast |  |  |
| `polars_core:7314` | `Column::wrapping_trunc_div_scalar` | `T`: num_traits::Num + num_traits::cast::NumCast |  |  |
| `polars_core:11219` | `Series::wrapping_trunc_div_scalar` | `T`: num_traits::Num + num_traits::cast::NumCast |  |  |

## callback audit (76)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3595` | `ChunkedArray::apply_amortized_generic` | `F`: FnMut(Option<AmortSeries>) -> Option<K> + Copy; `K`: ; `V`: PolarsDataType |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3467` | `ChunkedArray::apply_mut` | `F`: FnMut(&str) -> &str |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3469` | `ChunkedArray::apply_mut` | `F`: FnMut(&[u8]) -> &[u8] |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3457` | `ChunkedArray::apply_nonnull_values_generic` | `U`: PolarsDataType; `K`: ; `F`: FnMut(T::Physical) -> K |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3599` | `ChunkedArray::binary_zip_and_apply_amortized` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(Option<AmortSeries>, Option<T::Physical>, Op |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3462` | `ChunkedArray::cast_and_apply_in_place` | `F`: Fn(S::Native) -> S::Native + Copy; `S`: PolarsNumericType |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3597` | `ChunkedArray::for_each_amortized` | `F`: FnMut(Option<AmortSeries>) |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3603` | `ChunkedArray::try_apply_amortized` | `F`: FnMut(AmortSeries) -> PolarsResult<Series> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3596` | `ChunkedArray::try_apply_amortized_generic` | `F`: FnMut(Option<AmortSeries>) -> PolarsResult<Option<; `K`: ; `V`: PolarsDataType |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3460` | `ChunkedArray::try_apply_into_string_amortized` | `F`: FnMut(T::Physical, &mut String) -> Result<(), E>; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3458` | `ChunkedArray::try_apply_nonnull_values_generic` | `U`: PolarsDataType; `K`: ; `F`: FnMut(T::Physical) -> Result<K, E>; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3600` | `ChunkedArray::try_binary_zip_and_apply_amortized` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(Option<AmortSeries>, Option<T::Physical>, Op |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3601` | `ChunkedArray::try_zip_and_apply_amortized` | `T`: PolarsDataType; `F`: FnMut(Option<AmortSeries>, Option<T::Physical>) -> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3531` | `ChunkedArray::with_nullable_idx` | `T`: ; `F`: FnOnce(&IdxCa) -> T |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:3598` | `ChunkedArray::zip_and_apply_amortized` | `T`: PolarsDataType; `F`: FnMut(Option<AmortSeries>, Option<T::Physical>) -> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:1363` | `ChunkApply::apply` | `F`: Fn(Option<T>) -> Option<Self::FuncRet> + Copy |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:1364` | `ChunkApply::apply_to_slice` | `F`: Fn(Option<T>, &S) -> S; `S`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:1361` | `ChunkApply::apply_values` | `F`: Fn(T) -> Self::FuncRet + Copy |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:1318` | `ChunkSet::scatter_with` | `I`: IntoIterator<Item = IdxSize>; `F`: Fn(Option<A>) -> Option<B>; `Self`: Sized |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:806` | `apply_binary_kernel_broadcast` | `L`: PolarsDataType; `R`: PolarsDataType; `O`: PolarsDataType; `K`: Fn(&L::Array, &R::Array) -> O::Array; `LK`: Fn(L::Physical, &R::Array) -> O::Array; `RK`: Fn(&L::Array, R::Physical) -> O::Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:807` | `apply_binary_kernel_broadcast_owned` | `L`: PolarsDataType; `R`: PolarsDataType; `O`: PolarsDataType; `K`: Fn(L::Array, R::Array) -> O::Array; `LK`: Fn(L::Physical, R::Array) -> O::Array; `RK`: Fn(L::Array, R::Physical) -> O::Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:792` | `binary` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array, &U::Array) -> Arr; `Arr`: Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:783` | `binary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: BinaryFnMut<Option<T::Physical>, Option<U::Physica |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:784` | `binary_elementwise_for_each` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(Option<T::Physical>, Option<U::Physical>) |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:787` | `binary_elementwise_into_string_amortized` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(T::Physical, U::Physical, &mut String) |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:786` | `binary_elementwise_values` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(T::Physical, U::Physical) -> K; `K`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:789` | `binary_mut_values` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array, &U::Array) -> Arr; `Arr`: Array + StaticArray |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:790` | `binary_mut_with_options` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array, &U::Array) -> Arr; `Arr`: Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:793` | `binary_owned` | `L`: PolarsDataType; `R`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(L::Array, R::Array) -> Arr; `Arr`: Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:798` | `binary_to_series` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(&T::Array, &U::Array) -> Box<dyn Array> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:803` | `broadcast_binary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: BinaryFnMut<Option<T::Physical>, Option<U::Physica |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:805` | `broadcast_binary_elementwise_values` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(T::Physical, U::Physical) -> K; `K`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:804` | `broadcast_try_binary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(Option<T::Physical>, Option<U::Physical>) ->; `K`: ; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:802` | `ternary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `G`: PolarsDataType; `F`: TernaryFnMut<Option<T::Physical>, Option<U::Physic |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:794` | `try_binary` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array, &U::Array) -> Result<Arr, E>; `Arr`: Array; `E`: Error |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:785` | `try_binary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(Option<T::Physical>, Option<U::Physical>) ->; `K`: ; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:791` | `try_binary_mut_with_options` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array, &U::Array) -> Result<Arr, E>; `Arr`: Array; `E`: Error |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:799` | `try_binary_to_series` | `T`: PolarsDataType; `U`: PolarsDataType; `F`: FnMut(&T::Array, &U::Array) -> PolarsResult<Box<dy |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:801` | `try_ternary_elementwise` | `T`: PolarsDataType; `U`: PolarsDataType; `V`: PolarsDataType; `G`: PolarsDataType; `F`: FnMut(Option<T::Physical>, Option<U::Physical>, Op; `K`: ; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:775` | `try_unary_elementwise` | `T`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(Option<T::Physical>) -> Result<Option<K>, E>; `K`: ; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:777` | `try_unary_elementwise_values` | `T`: PolarsDataType; `V`: PolarsDataType; `F`: FnMut(T::Physical) -> Result<K, E>; `K`: ; `E`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:781` | `try_unary_mut_with_options` | `T`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array) -> Result<Arr, E>; `Arr`: Array + StaticArray; `E`: Error |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:797` | `try_unary_to_series` | `T`: PolarsDataType; `F`: FnMut(&T::Array) -> PolarsResult<Box<dyn Array>> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:773` | `unary_elementwise` | `T`: PolarsDataType; `V`: PolarsDataType; `F`: UnaryFnMut<Option<T::Physical>> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:776` | `unary_elementwise_values` | `T`: PolarsDataType; `V`: PolarsDataType; `F`: UnaryFnMut<T::Physical> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:771` | `unary_kernel` | `T`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array) -> Arr; `Arr`: Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:772` | `unary_kernel_owned` | `T`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(T::Array) -> Arr; `Arr`: Array |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:778` | `unary_mut_values` | `T`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array) -> Arr; `Arr`: Array + StaticArray |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:780` | `unary_mut_with_options` | `T`: PolarsDataType; `V`: PolarsDataType<Array = Arr>; `F`: FnMut(&T::Array) -> Arr; `Arr`: Array + StaticArray |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:971` | `lower_bound_chunks` | `T`: PolarsDataType; `F`: Fn(&T::Array, usize, &T::Physical) -> bool; `impl Iterator<Item = Opt`: Iterator<Item = Option<T::Physical>> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:4500` | `DataType::try_mutate_with` | `impl FnMut(Cow<'d, DataT`: FnMut(Cow<DataType>) -> PolarsResult<Cow<DataType> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7643` | `DataFrame::apply` | `F`: FnOnce(&Column) -> C; `C`: IntoColumn |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7644` | `DataFrame::apply_at_idx` | `F`: FnOnce(&Column) -> C; `C`: IntoColumn |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7662` | `DataFrame::pipe` | `F`: Fn(DataFrame) -> PolarsResult<B>; `B`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7663` | `DataFrame::pipe_mut` | `F`: Fn(&mut DataFrame) -> PolarsResult<B>; `B`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7664` | `DataFrame::pipe_with_args` | `F`: Fn(DataFrame, Args) -> PolarsResult<B>; `B`: ; `Args`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7646` | `DataFrame::try_apply` | `F`: FnOnce(&Series) -> PolarsResult<C>; `C`: IntoColumn |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:7645` | `DataFrame::try_apply_at_idx` | `F`: FnOnce(&Column) -> PolarsResult<C>; `C`: IntoColumn |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:8443` | `GroupBy::apply` | `F`: FnMut(DataFrame) -> PolarsResult<DataFrame> + Send |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:8444` | `GroupBy::apply_sliced` | `F`: FnMut(DataFrame) -> PolarsResult<DataFrame> + Send |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_core:8442` | `GroupBy::par_apply` | `F`: Fn(DataFrame) -> PolarsResult<DataFrame> + Send +  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_error:241` | `PolarsContext::with_context` | `F`: FnOnce() -> String |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_error:86` | `catch_polars_abort` | `R`: ; `F`: FnOnce() -> R + UnwindSafe |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_io:1708` | `PlCredentialProvider::from_func` | `impl Fn() ->     Pin<Box`: Fn() -> Pin<Box<dyn Future<Output = PolarsResult<( |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_io:561` | `PolarsObjectStore::exec_with_rebuild_retry_on_err` | `Fn`: FnMut(Cow<Arc<dyn object_store::ObjectStore>>) -> ; `Fut`: Future<Output = object_store::Result<O>>; `O`:  |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_io:5587` | `tune_with_concurrency_budget` | `F`: FnOnce() -> Fut; `Fut`: Future |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_io:5588` | `with_concurrency_budget` | `F`: FnOnce() -> Fut; `Fut`: Future |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_lazy:356` | `LazyFrame::map` | `F`: 'static + Fn(DataFrame) -> PolarsResult<DataFrame> |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_plan:11` | `PlanCallback::new` | `impl Fn(Args) -> PolarsR`: Fn(Args) -> PolarsResult<Out> + Send + Sync + 'sta |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_plan:5011` | `JoinTypeOptionsIR::compile` | `C`: FnOnce(&ExprIR) -> PolarsResult<Arc<dyn CrossJoinF |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_plan:15563` | `TreeWalker::apply_children` | `F`: FnMut(&Self, &Self::Arena) -> PolarsResult<VisitRe |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_plan:15564` | `TreeWalker::map_children` | `F`: FnMut(Self, &mut Self::Arena) -> PolarsResult<Self |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_schema:278` | `Schema::retain` | `F`: FnMut(&PlSmallStr, &Field) -> bool |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_schema:279` | `Schema::retain_mut` | `F`: FnMut(&mut PlSmallStr, &mut Field) -> bool |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_schema:254` | `Schema::sort_by_key` | `T`: Ord; `F`: FnMut(&PlSmallStr, &Field) -> T |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |
| `polars_schema:221` | `Schema::try_from_iter_check_duplicates` | `I`: IntoIterator<Item = PolarsResult<F>>; `F`: Into<(PlSmallStr, Field)>; `E`: Fn(&str) -> PolarsError |  | a closure bound needs a 0079 invocation/mutation/sink audit entry |

## blocked elsewhere (26)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:1843` | `ChunkFullNull::full_null` | `Self`: Sized |  | no wrapped implementor: polars_core::chunked_array::ChunkedArray |
| `polars_core:1669` | `ChunkUnique::unique` | `Self`: Sized |  | no wrapped implementor: polars_core::chunked_array::ChunkedArray |
| `polars_core:8423` | `GroupBy::select` | `I`: IntoIterator<Item = S>; `S`: Into<PlSmallStr> |  | lifetime owner: polars_core::frame::group_by::GroupBy |
| `polars_core:9921` | `IcebergSchema::try_from_arrow_fields_iter` | `I`: IntoIterator<Item = &Field> |  | owner has no public path: polars_core::schema::iceberg::IcebergSchema |
| `polars_io:2313` | `CsvReadOptions::with_path` | `P`: Into<PathBuf> |  | foreign type: path |
| `polars_io:5572` | `resolve_homedir` | `S`: AsRef<Path> + Sized |  | foreign type: path |
| `polars_lazy:530` | `JoinBuilder::left_on` | `E`: AsRef<[Expr]> |  | receiver consumes a non-Clone type: polars_lazy::frame::JoinBuilder |
| `polars_lazy:529` | `JoinBuilder::on` | `E`: AsRef<[Expr]> |  | receiver consumes a non-Clone type: polars_lazy::frame::JoinBuilder |
| `polars_lazy:531` | `JoinBuilder::right_on` | `E`: AsRef<[Expr]> |  | receiver consumes a non-Clone type: polars_lazy::frame::JoinBuilder |
| `polars_lazy:535` | `JoinBuilder::suffix` | `S`: Into<PlSmallStr> |  | receiver consumes a non-Clone type: polars_lazy::frame::JoinBuilder |
| `polars_ops:929` | `DataFrameJoinOps::full_join` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | no wrapped implementor:  |
| `polars_ops:927` | `DataFrameJoinOps::inner_join` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | no wrapped implementor:  |
| `polars_ops:924` | `DataFrameJoinOps::join` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | no wrapped implementor:  |
| `polars_ops:928` | `DataFrameJoinOps::left_join` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | no wrapped implementor:  |
| `polars_plan:626` | `DslBuilder::scan_csv` | `impl Into<Arc<CsvReadOpt`: Into<Arc<CsvReadOptions>> |  | foreign type: options |
| `polars_plan:226` | `CategoricalNameSpace::to` | `impl Into<DataTypeExpr>`: Into<DataTypeExpr> |  | receiver consumes a non-Clone type: polars_plan::dsl::cat::Categorical |
| `polars_plan:2891` | `cum_fold_exprs` | `E`: AsRef<[Expr]> |  | generic type: f |
| `polars_plan:2890` | `cum_reduce_exprs` | `E`: AsRef<[Expr]> |  | generic type: f |
| `polars_plan:2887` | `fold_exprs` | `E`: AsRef<[Expr]> |  | generic type: f |
| `polars_plan:2894` | `max_horizontal` | `E`: AsRef<[Expr]> |  | name taken in polars:: by: polars_ops::series::ops::horizontal::max_ho |
| `polars_plan:2897` | `mean_horizontal` | `E`: AsRef<[Expr]> |  | name taken in polars:: by: polars_ops::series::ops::horizontal::mean_h |
| `polars_plan:2895` | `min_horizontal` | `E`: AsRef<[Expr]> |  | name taken in polars:: by: polars_ops::series::ops::horizontal::min_ho |
| `polars_plan:2889` | `reduce_exprs` | `E`: AsRef<[Expr]> |  | generic type: f |
| `polars_plan:2896` | `sum_horizontal` | `E`: AsRef<[Expr]> |  | name taken in polars:: by: polars_ops::series::ops::horizontal::sum_ho |
| `polars_plan:2972` | `ListNameSpace::agg` | `E`: Into<Expr> |  | receiver consumes a non-Clone type: polars_plan::dsl::list::ListNameSp |
| `polars_plan:2971` | `ListNameSpace::eval` | `E`: Into<Expr> |  | receiver consumes a non-Clone type: polars_plan::dsl::list::ListNameSp |

## refused: return-only (20)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:2890` | `ChunkedCollectInferIterExt::try_collect_ca` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:2891` | `ChunkedCollectInferIterExt::try_collect_ca_trusted` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> + TrustedLen |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:2883` | `ChunkedCollectIterExt::try_collect_ca_like` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:2885` | `ChunkedCollectIterExt::try_collect_ca_trusted_like` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> + TrustedLen |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:2884` | `ChunkedCollectIterExt::try_collect_ca_trusted_with_dtype` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> + TrustedLen |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:2882` | `ChunkedCollectIterExt::try_collect_ca_with_dtype` | `U`: ; `E`: ; `Self`: Iterator<Item = Result<U, E>> |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:3181` | `ChunkedCollectParIterExt::collect_ca_with_dtype` | `B`: FromParIterWithDtype<Self::Item>; `Self`: Sized |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:1916` | `ChunkApplyKernel::apply_kernel_cast` | `S`: PolarsDataType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:1405` | `ChunkQuantile::quantiles` | `T`: Clone |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:4157` | `AnyValue::extract` | `T`: num_traits::cast::NumCast + IsFloat |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:4159` | `AnyValue::try_extract` | `T`: num_traits::cast::NumCast + IsFloat |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:7221` | `Column::cat` | `T`: PolarsCategoricalType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:7197` | `Column::try_cat` | `T`: PolarsCategoricalType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11287` | `Series::cat` | `T`: PolarsCategoricalType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11334` | `Series::max` | `T`: num_traits::cast::NumCast + IsFloat |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11333` | `Series::min` | `T`: num_traits::cast::NumCast + IsFloat |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11332` | `Series::sum` | `T`: num_traits::cast::NumCast + IsFloat |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11307` | `Series::take_inner` | `T`: PolarsPhysicalType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_core:11262` | `Series::try_cat` | `T`: PolarsCategoricalType |  | the generic is chosen only by a return annotation the script cannot ex |
| `polars_io:6599` | `decode_json_response` | `T`: serde_core::de::Deserialize |  | the generic is chosen only by a return annotation the script cannot ex |

## refused: foreign element (26)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3578` | `ChunkedArray::from_chunk_iter` | `I`: IntoIterator; `T`: PolarsDataType<Array = <I as IntoIterator>::Item> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3579` | `ChunkedArray::from_chunk_iter_like` | `I`: IntoIterator; `T`: PolarsDataType<Array = <I as IntoIterator>::Item> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3580` | `ChunkedArray::try_from_chunk_iter` | `I`: IntoIterator<Item = Result<A, E>>; `A`: Array; `E`: ; `T`: PolarsDataType<Array = A> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3576` | `ChunkedArray::with_chunk` | `A`: Array; `T`: PolarsDataType<Array = A> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3577` | `ChunkedArray::with_chunk_like` | `A`: Array; `T`: PolarsDataType<Array = A> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3173` | `FromParIterWithDtype::from_par_iter_with_dtype` | `I`: rayon::iter::IntoParallelIterator<Item = K>; `Self`: Sized |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3179` | `list_from_par_iter` | `I`: rayon::iter::IntoParallelIterator<Item = Option<Se |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3180` | `try_list_from_par_iter` | `I`: rayon::iter::IntoParallelIterator<Item = PolarsRes |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3294` | `Logical::from_duration` | `I`: IntoIterator<Item = chrono::Duration> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3295` | `Logical::from_duration_options` | `I`: IntoIterator<Item = Option<chrono::Duration>> |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3270` | `Logical::from_naive_date` | `I`: IntoIterator<Item = chrono::naive::date::NaiveDate |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3273` | `Logical::from_naive_date_options` | `I`: IntoIterator<Item = Option<chrono::naive::date::Na |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3283` | `Logical::from_naive_datetime` | `I`: IntoIterator<Item = chrono::naive::datetime::Naive |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3284` | `Logical::from_naive_datetime_options` | `I`: IntoIterator<Item = Option<chrono::naive::datetime |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3301` | `Logical::from_naive_time` | `I`: IntoIterator<Item = chrono::naive::time::NaiveTime |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:3302` | `Logical::from_naive_time_options` | `I`: IntoIterator<Item = Option<chrono::naive::time::Na |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:4180` | `AnyValue::hash_impl` | `H`: Hasher |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:7686` | `DataFrame::deserialize_from_reader` | `T`: Read + Seek |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_core:11214` | `Series::deserialize_from_reader` | `T`: Read + Seek |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_io:3952` | `IpcWriterOptions::to_writer` | `W`: Write |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_io:5313` | `ParquetWriteOptions::to_writer` | `F`: Write |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_io:5492` | `ParquetWriter::new` | `W`: Write |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_io:6590` | `get_reader_bytes` | `R`: Read + MmapBytesReader + Sized |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_plan:666` | `DslBuilder::map` | `F`: DataFrameUdf + 'static |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_plan:1028` | `new_column_udf` | `F`: AnonymousColumnsUdf + 'static |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |
| `polars_plan:6972` | `UserDefinedFunction::new` | `impl AnonymousColumnsUdf`: AnonymousColumnsUdf + 'static |  | a chrono, rayon, io, path, serde or Arrow value has no script conversi |

## refused: Self bound (15)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3500` | `ChunkedArray::head` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:3499` | `ChunkedArray::limit` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:3501` | `ChunkedArray::tail` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:2889` | `ChunkedCollectInferIterExt::collect_ca_trusted` | `Self`: TrustedLen |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:2881` | `ChunkedCollectIterExt::collect_ca_trusted_like` | `Self`: TrustedLen |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:2880` | `ChunkedCollectIterExt::collect_ca_trusted_with_dtype` | `Self`: TrustedLen |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:1821` | `ChunkFillNullValue::fill_null_with_values` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:1873` | `ChunkFilter::filter` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:1829` | `ChunkFull::full` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:1319` | `ChunkSet::set` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:1288` | `ChunkTake::take` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:5089` | `PolarsDataType::get_static_dtype` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_core:10660` | `IntoSeries::into_series` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_io:6087` | `SerReader::set_rechunk` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |
| `polars_io:6089` | `SerWriter::new` | `Self`: Sized |  | a trait method whose only generic is Self (iterator extension or marke |

## refused: bound (15)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:7609` | `DataFrame::drop_nulls` | `S`:  |  | today's inference admits Into/AsRef/IntoIterator only; bounds [''] |
| `polars_core:10193` | `ensure_matching_schema` | `F`: ; `M`:  |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['', ''] |
| `polars_core:11228` | `Series::from_array` | `A`: ParameterFreeDtypeStaticArray |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['polars |
| `polars_core:11236` | `Series::try_new` | `T`:  |  | today's inference admits Into/AsRef/IntoIterator only; bounds [''] |
| `polars_error:246` | `map_err` | `E`: Error |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['core:: |
| `polars_io:2341` | `CsvReadOptions::into_reader_with_file_handle` | `R`: MmapBytesReader |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['polars |
| `polars_io:7617` | `merge_sorted_to_schema_order` | `F`: ; `M`:  |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['', ''] |
| `polars_io:7005` | `Writable::write_all_owned` | `T`: AsRef<[u8]> + Default + Drop |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['core:: |
| `polars_io:6929` | `AsyncWritable::write_all_owned` | `T`: AsRef<[u8]> + Default + Drop |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['core:: |
| `polars_plan:2941` | `lit` | `L`: Literal |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['polars |
| `polars_plan:15566` | `TreeWalker::rewrite` | `R`: RewritingVisitor<Node = Self, Arena = Self::Arena> |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['polars |
| `polars_plan:15565` | `TreeWalker::visit` | `V`: Visitor<Node = Self, Arena = Self::Arena> |  | today's inference admits Into/AsRef/IntoIterator only; bounds ['polars |
| `polars_schema:217` | `Schema::from_iter_check_duplicates` | `I`: IntoIterator<Item = F>; `F`: Into<(PlSmallStr, Field)> |  | the item converts into a tuple holding a wrapped value (`Into<(PlSmall |
| `polars_schema:253` | `Schema::hstack` | `impl Into<(PlSmallStr, F`: Into<(PlSmallStr, Field)>; `impl IntoIterator<Item =`: IntoIterator<Item = impl Into<(PlSmallStr, Field)> |  | the item converts into a tuple holding a wrapped value (`Into<(PlSmall |
| `polars_schema:252` | `Schema::hstack_mut` | `impl Into<(PlSmallStr, F`: Into<(PlSmallStr, Field)>; `impl IntoIterator<Item =`: IntoIterator<Item = impl Into<(PlSmallStr, Field)> |  | the item converts into a tuple holding a wrapped value (`Into<(PlSmall |

## refused: iterator item (10)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:7636` | `DataFrame::rename_many` | `impl Iterator<Item = (&'`: Iterator<Item = (&str, PlSmallStr)> |  | item ['(&str, polars_utils::pl_str::PlSmallStr)'] has no script conver |
| `polars_core:7625` | `DataFrame::select_to_vec` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | items that are themselves anonymous generics or results have no script |
| `polars_core:7637` | `DataFrame::sort` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | items that are themselves anonymous generics or results have no script |
| `polars_core:7638` | `DataFrame::sort_in_place` | `impl AsRef<str>`: AsRef<str>; `impl IntoIterator<Item =`: IntoIterator<Item = impl AsRef<str>> |  | items that are themselves anonymous generics or results have no script |
| `polars_core:7571` | `DataFrame::try_from_rows_iter_and_schema` | `I`: Iterator<Item = PolarsResult<&Row>> |  | items that are themselves anonymous generics or results have no script |
| `polars_core:8871` | `infer_schema` | `impl Into<PlSmallStr>`: Into<PlSmallStr>; `impl Into<DataType>`: Into<DataType>; `impl Iterator<Item = Vec`: Iterator<Item = Vec<(impl Into<PlSmallStr>, impl I |  | items that are themselves anonymous generics or results have no script |
| `polars_io:302` | `CloudOptions::from_untyped_config` | `I`: IntoIterator<Item = (impl AsRef<str>, impl Into<St; `impl AsRef<str>`: AsRef<str>; `impl Into<String>`: Into<String> |  | items that are themselves anonymous generics or results have no script |
| `polars_io:568` | `PolarsObjectStore::build_buffered_ranges_stream` | `T`: Iterator<Item = Range<usize>> |  | item ['core::ops::range::Range<usize>'] has no script conversion |
| `polars_io:3645` | `init_entries_from_uri_list` | `impl ExactSizeIterator<I`: ExactSizeIterator<Item = PlRefPath> + Send + 'stat |  | item ['polars_utils::pl_path::PlRefPath'] has no script conversion |
| `polars_plan:1465` | `Expr::try_map_n_ary` | `impl Into<FunctionExpr>`: Into<FunctionExpr>; `impl IntoIterator<Item =`: IntoIterator<Item = PolarsResult<Expr>> |  | items that are themselves anonymous generics or results have no script |

## refused: unbounded generic (9)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:7162` | `Column::new` | `T`: ; `Phantom`: Sized |  | a generic with no bound cannot be chosen |
| `polars_core:7667` | `DataFrame::unique` | `I`: ; `S`:  |  | a generic with no bound cannot be chosen |
| `polars_error:247` | `to_compute_err` | `impl Display`: Display |  | a generic with no bound cannot be chosen |
| `polars_io:7620` | `merge_sorted_to_schema_order_impl` | `T`: ; `O`: Extend<T> |  | a generic with no bound cannot be chosen |
| `polars_io:3881` | `IpcReadOptions::with_predicate` | `impl Into<Option<Arc<dyn`: Into<Option<Arc<dyn PhysicalIoExpr>>> |  | a generic with no bound cannot be chosen |
| `polars_io:7550` | `OptIOMetrics::record_bytes_tx` | `F`: Future<Output = O>; `O`:  |  | a generic with no bound cannot be chosen |
| `polars_io:7549` | `OptIOMetrics::record_io_read` | `F`: Future<Output = O>; `O`:  |  | a generic with no bound cannot be chosen |
| `polars_lazy:927` | `LazyFileListReader::with_n_rows` | `impl Into<Option<usize>>`: Into<Option<usize>> |  | a generic with no bound cannot be chosen |
| `polars_lazy:928` | `LazyFileListReader::with_row_index` | `impl Into<Option<RowInde`: Into<Option<RowIndex>> |  | a generic with no bound cannot be chosen |

## refused: free element generic (3)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3152` | `arg_max_opt_iter` | `T`: Ord; `I`: IntoIterator<Item = Option<T>> |  | the item type is a free generic (`T: Ord`) with no Polars family to in |
| `polars_core:3151` | `arg_min_opt_iter` | `T`: Ord; `I`: IntoIterator<Item = Option<T>> |  | the item type is a free generic (`T: Ord`) with no Polars family to in |
| `polars_ops:83` | `ChunkedSet::scatter` | `V`: IntoIterator<Item = Option<T>> |  | the item type is a free generic (`T: Ord`) with no Polars family to in |

## refused: unchecked precondition (1)

Polars enforces the single-chunk receiver and the chunk-length sum with debug_assert! only and slices unchecked (polars-core 0.55.2 chunked_array/mod.rs:851-866); a binding must validate both or stay refused (probe B)

| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3704` | `ChunkedArray::match_chunks` | `I`: Iterator<Item = usize> |  | generic owner |

## refused: iterator of borrowed wrappers (2)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:3640` | `ChunkedArray::from_series` | `I`: ExactSizeIterator<Item = &Series> + Clone |  | items borrow wrapped values (`&Series`, `&Row`); needs a borrow-lifeti |
| `polars_core:7570` | `DataFrame::from_rows_iter_and_schema` | `I`: Iterator<Item = &Row> |  | items borrow wrapped values (`&Series`, `&Row`); needs a borrow-lifeti |

## refused: iterator without item (1)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_schema:274` | `Schema::try_project` | `I`: IntoIterator |  | an `IntoIterator` bound with no item type cannot be chosen |

## refused: other (3)



| key | canonical path | generics | concrete | note |
|---|---|---|---|---|
| `polars_core:7596` | `DataFrame::set_column_names` | `T`: AsRef<str> |  | slice of borrows: new_names |
| `polars_io:6592` | `columns_to_projection` | `T`: AsRef<str> |  | slice of borrows: columns |
| `polars_plan:647` | `DslBuilder::group_by` | `E`: AsRef<[Expr]> |  | generic type: apply |

## Unresolved receiver pairs (family census), by method

| method | pairs | decision |
|---|---:|---|
| `ChunkedArray::apply_amortized_generic` | 16 | callback audit |
| `ChunkedArray::apply_nonnull_values_generic` | 16 | callback audit |
| `ChunkedArray::binary_zip_and_apply_amortized` | 16 | callback audit |
| `ChunkedArray::cast_and_apply_in_place` | 16 | callback audit |
| `ChunkedArray::try_apply_amortized_generic` | 16 | callback audit |
| `ChunkedArray::try_apply_into_string_amortized` | 16 | callback audit |
| `ChunkedArray::try_apply_nonnull_values_generic` | 16 | callback audit |
| `ChunkedArray::try_binary_zip_and_apply_amortized` | 16 | callback audit |
| `ChunkedArray::try_zip_and_apply_amortized` | 16 | callback audit |
| `ChunkedArray::with_nullable_idx` | 16 | callback audit |
| `ChunkedArray::zip_and_apply_amortized` | 16 | callback audit |
| `ChunkedArray::retain_flags_from` | 16 | candidate: family instantiation |
| `Logical::from_str_iter` | 4 | candidate: iterator input, borrowed items (per family) |
| `ChunkedArray::lhs_div` | 16 | policy: numeric scalar |
| `ChunkedArray::lhs_rem` | 16 | policy: numeric scalar |
| `ChunkedArray::lhs_sub` | 16 | policy: numeric scalar |
| `ChunkedArray::head` | 16 | refused: Self bound |
| `ChunkedArray::limit` | 16 | refused: Self bound |
| `ChunkedArray::tail` | 16 | refused: Self bound |
| `ChunkedArray::from_chunk_iter` | 16 | refused: foreign element |
| `ChunkedArray::from_chunk_iter_like` | 16 | refused: foreign element |
| `ChunkedArray::try_from_chunk_iter` | 16 | refused: foreign element |
| `ChunkedArray::with_chunk` | 16 | refused: foreign element |
| `ChunkedArray::with_chunk_like` | 16 | refused: foreign element |
| `Logical::from_duration` | 4 | refused: foreign element |
| `Logical::from_duration_options` | 4 | refused: foreign element |
| `Logical::from_naive_date` | 4 | refused: foreign element |
| `Logical::from_naive_date_options` | 4 | refused: foreign element |
| `Logical::from_naive_datetime` | 4 | refused: foreign element |
| `Logical::from_naive_datetime_options` | 4 | refused: foreign element |
| `Logical::from_naive_time` | 4 | refused: foreign element |
| `Logical::from_naive_time_options` | 4 | refused: foreign element |
| `ChunkedArray::from_series` | 16 | refused: iterator of borrowed wrappers |
| `ChunkedArray::match_chunks` | 16 | refused: unchecked precondition |
