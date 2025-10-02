#[cfg(test)]
mod tests {
    use crate::Formatter;

    #[test]
    fn test_format_simple_function() {
        let input = r#"fn main(){print("Hello, world!");}"#;

        let expected = r#"fn main() {
  print("Hello, world!");
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_function_with_params() {
        let input = r#"fn add(x:i32,y:i32)->i32{return x+y;}"#;

        let expected = r#"fn add(x: i32, y: i32) -> i32 {
  return x + y;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_let_and_var() {
        let input = r#"fn main(){let x=10;let y:i32=20;var z=30;}"#;

        let expected = r#"fn main() {
  let x = 10;
  let y: i32 = 20;
  var z = 30;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_if_else() {
        let input = r#"fn main(){if(x>10){print("greater");}else{print("less");}}"#;

        let expected = r#"fn main() {
  if (x > 10) {
    print("greater");
  } else {
    print("less");
  }}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_while_loop() {
        let input = r#"fn main(){while(x<10){x=x+1;}}"#;

        let expected = r#"fn main() {
  while (x < 10) {
    x = x + 1;
  }}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_nested_expressions() {
        let input = r#"fn main(){let result=(1+2)*3;let complex=add(a=multiply(x=1,y=2),b=subtract(a=5,b=3));}"#;

        let expected = r#"fn main() {
  let result = 1 + 2 * 3;
  let complex = add(a = multiply(x = 1, y = 2), b = subtract(a = 5, b = 3));
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_boolean_operators() {
        let input = r#"fn main(){let a=true and false;let b=not true or false;}"#;

        let expected = r#"fn main() {
  let a = true and false;
  let b = not true or false;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_string_literals() {
        let input = r#"fn main(){let s="hello\nworld";print("tab\there");}"#;

        let expected = r#"fn main() {
  let s = "hello\nworld";
  print("tab\there");
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_string_interpolation() {
        let input = r#"fn main(){let name="World";print("Hello, ${name}!");print("Sum: ${2+2}");}"#;

        let expected = r#"fn main() {
  let name = "World";
  print("Hello, ${name}!");
  print("Sum: ${2 + 2}");
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_multiple_functions() {
        let input = r#"fn add(x:i32,y:i32)->i32{return x+y;}fn main(){let result=add(x=5,y=3);}"#;

        let expected = r#"fn add(x: i32, y: i32) -> i32 {
  return x + y;
}

fn main() {
  let result = add(x = 5, y = 3);
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_complex_nested_code() {
        let input = r#"fn main(){if(x>0){let y=x*2;while(y>0){print("${y}");y=y-1;}}else{print("negative");}}"#;

        let expected = r#"fn main() {
  if (x > 0) {
    let y = x * 2;
    while (y > 0) {
      print("${y}");
      y = y - 1;
    }  } else {
    print("negative");
  }}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_formatter_idempotence() {
        let input = r#"fn main() {
  let x = 10;
  if (x > 5) {
    print("Greater than 5");
  } else {
    print("Less than or equal to 5");
  }
}
"#;

        // Format once
        let first_format = Formatter::format(input).unwrap();
        // Format again
        let second_format = Formatter::format(&first_format).unwrap();

        // Should be identical after second format
        assert_eq!(first_format, second_format);
    }

    #[test]
    fn test_format_all_operators() {
        let input = r#"fn main(){let a=1+2;let b=3-4;let c=5*6;let d=7/8;let e=9%10;let f=a==b;let g=a!=b;let h=a<b;let i=a<=b;let j=a>b;let k=a>=b;}"#;

        let expected = r#"fn main() {
  let a = 1 + 2;
  let b = 3 - 4;
  let c = 5 * 6;
  let d = 7 / 8;
  let e = 9 % 10;
  let f = a == b;
  let g = a != b;
  let h = a < b;
  let i = a <= b;
  let j = a > b;
  let k = a >= b;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_unary_operators() {
        let input = r#"fn main(){let a=not true;let b=-5;let c=not(x and y);}"#;

        let expected = r#"fn main() {
  let a = not true;
  let b = -5;
  let c = not x and y;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_assignment_expressions() {
        let input = r#"fn main(){var x=10;x=20;x=x+1;}"#;

        let expected = r#"fn main() {
  var x = 10;
  x = 20;
  x = x + 1;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_format_empty_return() {
        let input = r#"fn test(){return;}"#;

        let expected = r#"fn test() {
  return;
}
"#;

        let result = Formatter::format(input).unwrap();
        assert_eq!(result, expected);
    }
}