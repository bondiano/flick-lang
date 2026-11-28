use super::tests::{errors, sexp};

#[test]
fn attributes_stand_before_declarations_and_constructors() {
    let src = "@lang int\ntype Int\n@lang flat_map\nflat_map : a -> a\nclass Monad m =\n  @lang pure\n  pure : a -> m a\ntype Option a = @lang none None | @lang some Some a\n";
    assert_eq!(
        sexp(src),
        "(Attribute @ lang int)\n(TypeDecl type Int)\n(Attribute @ lang flat_map)\n\
         (Signature flat_map : (ArrowType (NameType a) -> (NameType a)))\n\
         (ClassDecl class Monad (TypeParam m) = (Members (Attribute @ lang pure) \
         (Signature pure : (ArrowType (NameType a) -> (AppType (NameType m) (NameType a))))))\n\
         (TypeDecl type Option (TypeParam a) = (Variant (Attribute @ lang none) None) | \
         (Variant (Attribute @ lang some) Some (NameType a)))"
    );
}

#[test]
fn record_field_attributes_and_namespaced_attributes_parse() {
    let inline = "type alias User = { @skip x : Int; @default -1 y : Int; @meta.rename \"uid\" id : String }\n";
    assert_eq!(errors(inline), Vec::<String>::new());
    assert_eq!(
        sexp(inline),
        "(TypeDecl type alias User = (RecordType { \
         (RecordTypeField (Attribute @ skip) x : (NameType Int)) ; \
         (RecordTypeField (Attribute @ default - 1) y : (NameType Int)) ; \
         (RecordTypeField (Attribute @ meta . rename \"uid\") id : (NameType String)) }))"
    );

    let multiline = "\
@meta.prefix \"v1\"
type User = User {
  @rename \"user_id\"
  @default \"anon\"
  userName : String
  @default 0
  retries : Int
}
";
    assert_eq!(errors(multiline), Vec::<String>::new());
    assert_eq!(
        sexp(multiline),
        "(Attribute @ meta . prefix \"v1\")\n\
         (TypeDecl type User = (Variant User (RecordType { \
         (RecordTypeField (Attribute @ rename \"user_id\") (Attribute @ default \"anon\") userName : (NameType String)) \
         (RecordTypeField (Attribute @ default 0) retries : (NameType Int)) })))"
    );

    // Spaceless record arrow with leading attribute still triggers S007 safe fix.
    assert_eq!(errors("f : A ->{@skip x : Int}\n"), ["FLICK-S007 {"]);

    // Core attributes misplaced on record fields are rejected with FLICK-S014.
    let bad_core = "\
type Bad = Bad {
  @cell
  a : Int
  @derive
  b : Int
  @maker
  c : Int
  @lang int
  d : Int
}
";
    assert_eq!(
        errors(bad_core),
        [
            "FLICK-S014 @cell",
            "FLICK-S014 @derive",
            "FLICK-S014 @maker",
            "FLICK-S014 @lang int",
        ]
    );

    // Core attributes reject numeric and trailing arguments with FLICK-S014.
    let bad_args = "\
@derive 42
class C a =
  c : a -> Int

@lang int 42
type T1

@sync 42
service S1 =
  s1 : Unit -> Unit

@cell 42
type T2

@maker -1
let mk x = x

@owner 0
service S2 =
  s2 : Unit -> Unit

@stores 1
type T3

@implicit 1
instance C Int =
  let c x = x

@label atom 1
type T4
";
    assert_eq!(
        errors(bad_args),
        [
            "FLICK-S014 @derive 42",
            "FLICK-S014 @lang int 42",
            "FLICK-S014 @sync 42",
            "FLICK-S014 @cell 42",
            "FLICK-S014 @maker -1",
            "FLICK-S014 @owner 0",
            "FLICK-S014 @stores 1",
            "FLICK-S014 @implicit 1",
            "FLICK-S014 @label atom 1",
        ]
    );
}
