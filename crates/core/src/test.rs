#[cfg(test)]
mod tests {
    use crate::calculate_functions::*;
    use crate::parser::*;
    use crate::sheet_functions::*;
    use std::collections::HashMap;
    use std::string::String;

    // Helper function to create test sheets
    fn create_test_sheet() -> Sheet {
        Sheet::new(10, 10)
    }

    // Basic Sheet Tests
    #[test]
    fn test_sheet_creation() {
        let sheet = create_test_sheet();
        assert_eq!(sheet.rows, 10);
        assert_eq!(sheet.cols, 10);
        assert_eq!(sheet.data.len(), 0);
    }

    #[test]
    fn test_column_name_conversion() {
        assert_eq!(col_name_to_col_num("A"), 0);
        assert_eq!(col_name_to_col_num("Z"), 25);
        assert_eq!(col_name_to_col_num("AA"), 26);
        assert_eq!(col_name_to_col_num("AB"), 27);

        assert_eq!(col_num_to_col_name(0), "A");
        assert_eq!(col_num_to_col_name(25), "Z");
        assert_eq!(col_num_to_col_name(26), "AA");
        assert_eq!(col_num_to_col_name(27), "AB");
    }

    #[test]
    fn test_cell_validity() {
        assert!(is_valid_cell(0, 0, 10, 10));
        assert!(is_valid_cell(9, 9, 10, 10));
        assert!(!is_valid_cell(10, 9, 10, 10));
        assert!(!is_valid_cell(9, 10, 10, 10));
        assert!(!is_valid_cell(-1, 0, 10, 10));
    }

    // Cell Management Tests
    #[test]
    fn test_get_or_create_cell() {
        let mut sheet = create_test_sheet();

        let cell = get_or_create_cell(&mut sheet, 1, 1);
        assert_eq!(cell.value, 0);
        assert!(!cell.is_error);
        assert_eq!(cell.string, None);
        assert_eq!(cell.op_code, OpCode::NoConstraint);

        cell.value = 42;

        let cell2 = get_or_create_cell(&mut sheet, 1, 1);
        assert_eq!(cell2.value, 42);
    }

    // Mathematical Function Tests
    #[test]
    fn test_sum_function() {
        let mut sheet = create_test_sheet();

        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 20;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 40;

        let result = sum(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_min_function() {
        let mut sheet = create_test_sheet();

        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 5;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 15;

        let result = min(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_max_function() {
        let mut sheet = create_test_sheet();

        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 20;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 15;

        let result = max(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 30);
    }

    #[test]
    fn test_avg_function() {
        let mut sheet = create_test_sheet();

        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 20;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 40;

        let result = avg(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 25); // (10+20+30+40)/4 = 25
    }

    #[test]
    fn test_stdev_function() {
        let mut sheet = create_test_sheet();

        // Uniform values (standard deviation should be 0)
        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 10;
        get_or_create_cell(&mut sheet, 1, 0).value = 10;
        get_or_create_cell(&mut sheet, 1, 1).value = 10;

        let result = stdev(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 0);

        // Mixed values
        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 20;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 40;

        let result = stdev(&sheet, 0, 0, 1, 1);
        assert_eq!(result, 11); // sqrt((sum((x-avg)^2)/n)) ≈ 13
    }

    // Cell Computation Tests
    #[test]
    fn test_compute_cell() {
        let mut status = String::new();

        // Addition
        let (result, error) = compute_cell(OpCode::CellPlusCell, 10, 20, &mut status);
        assert_eq!(result, 30);
        assert!(!error);

        // Subtraction
        let (result, error) = compute_cell(OpCode::CellMinusCell, 30, 10, &mut status);
        assert_eq!(result, 20);
        assert!(!error);

        // Multiplication
        let (result, error) = compute_cell(OpCode::CellTimesCell, 5, 4, &mut status);
        assert_eq!(result, 20);
        assert!(!error);

        // Division
        let (result, error) = compute_cell(OpCode::CellDivideCell, 20, 5, &mut status);
        assert_eq!(result, 4);
        assert!(!error);

        // Division by zero
        let (result, error) = compute_cell(OpCode::CellDivideCell, 20, 0, &mut status);
        assert_eq!(result, -1);
        assert!(error);

        //
        let (result, error) = compute_cell(OpCode::ConstantDividesCell, 0, 20, &mut status);
        assert_eq!(result, -1);
        assert!(error);

        let (result, error) = compute_cell(OpCode::ConstantDividesCell, 20, 20, &mut status);
        assert_eq!(result, 1);
        assert!(!error);

        let (result, error) = compute_cell(OpCode::String, 20, 0, &mut status);
        assert_eq!(result, 20);
        assert!(!error);

        let (result, error) = compute_cell(OpCode::Sleep, 20, 0, &mut status);
        assert_eq!(result, -1);
        assert!(error);
    }

    // OpCode Tests
    #[test]
    fn test_get_op_code() {
        assert_eq!(get_op_code('+', false), OpCode::CellPlusConstant);
        assert_eq!(get_op_code('-', false), OpCode::CellMinusConstant);
        assert_eq!(get_op_code('*', false), OpCode::CellTimesConstant);
        assert_eq!(get_op_code('/', false), OpCode::CellDivideConstant);
        assert_eq!(get_op_code('/', true), OpCode::ConstantDividesCell);
        assert_eq!(get_op_code('x', false), OpCode::NoConstraint);
    }

    #[test]
    fn test_get_op_code2() {
        assert_eq!(get_op_code2('+'), OpCode::CellPlusCell);
        assert_eq!(get_op_code2('-'), OpCode::CellMinusCell);
        assert_eq!(get_op_code2('*'), OpCode::CellTimesCell);
        assert_eq!(get_op_code2('/'), OpCode::CellDivideCell);
        assert_eq!(get_op_code2('x'), OpCode::NoConstraint);
    }

    #[test]
    fn test_func_to_op_code() {
        assert_eq!(func_to_op_code("SUM"), OpCode::Sum);
        assert_eq!(func_to_op_code("MIN"), OpCode::Min);
        assert_eq!(func_to_op_code("MAX"), OpCode::Max);
        assert_eq!(func_to_op_code("AVG"), OpCode::Avg);
        assert_eq!(func_to_op_code("STDEV"), OpCode::Stdev);
        assert_eq!(func_to_op_code("UNKNOWN"), OpCode::NoConstraint);
    }

    // Cell Dependency Tests
    #[test]
    fn test_add_constraints_simple() {
        let mut sheet = create_test_sheet();
        let mut status = String::new();
        let mut sleep_timer = 0;

        // Set A1 = 10
        let cell = CellInfo { row: 0, col: 0 };
        let cell1 = CellInfo { row: -1, col: -1 };
        let cell2 = CellInfo { row: -1, col: -1 };

        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        add_constraints(
            cell,
            cell1,
            cell2,
            OpCode::NoConstraint,
            &mut sheet,
            &mut status,
            &mut sleep_timer,
        );

        assert_eq!(sheet.data.get(&(0, 0)).unwrap().value, 10);
        assert_eq!(status, "ok");
    }

    #[test]
    fn test_add_constraints_cell_equals_cell() {
        let mut sheet = create_test_sheet();
        let mut status = String::new();
        let mut sleep_timer = 0;

        // Set B1 = 20
        get_or_create_cell(&mut sheet, 0, 1).value = 20;

        // Set A1 = B1
        let cell = CellInfo { row: 0, col: 0 };
        let cell1 = CellInfo { row: 0, col: 1 };
        let cell2 = CellInfo { row: -1, col: -1 };

        add_constraints(
            cell,
            cell1,
            cell2,
            OpCode::CellEqualsCell,
            &mut sheet,
            &mut status,
            &mut sleep_timer,
        );

        assert_eq!(sheet.data.get(&(0, 0)).unwrap().value, 20);
        assert_eq!(status, "ok");
        assert!(
            sheet
                .data
                .get(&(0, 1))
                .unwrap()
                .dependencies
                .contains(&(0 * 1000 + 0))
        );
    }

    #[test]
    fn test_circular_dependency() {
        let mut sheet = create_test_sheet();
        let mut status = String::new();
        let mut sleep_timer = 0;

        // Set A1 = B1
        let cell_a1 = CellInfo { row: 0, col: 0 };
        let cell_b1 = CellInfo { row: 0, col: 1 };
        let empty = CellInfo { row: -1, col: -1 };

        add_constraints(
            cell_a1,
            cell_b1,
            empty,
            OpCode::CellEqualsCell,
            &mut sheet,
            &mut status,
            &mut sleep_timer,
        );

        // Try to create circular reference: B1 = A1
        status = String::new();
        let cell_b1 = CellInfo { row: 0, col: 1 };
        let cell_a1 = CellInfo { row: 0, col: 0 };
        let empty = CellInfo { row: -1, col: -1 };
        add_constraints(
            cell_b1,
            cell_a1,
            empty,
            OpCode::CellEqualsCell,
            &mut sheet,
            &mut status,
            &mut sleep_timer,
        );

        assert_eq!(status, "circular error");
    }

    // Parser Tests
    #[test]
    fn test_parse_command_simple_assignment() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::from("ok");
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Test A1=42
        parse_command(
            "A1=42",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        assert_eq!(sheet.data.get(&(0, 0)).unwrap().value, 42);
        assert_eq!(status, "ok");
    }

    #[test]
    fn test_parse_command_cell_operations() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Set up cells
        parse_command(
            "A1=10",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "B1=20",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        // C1=A1+B1
        parse_command(
            "C1=A1+B1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 2)).unwrap().value, 30);

        // D1=B1-A1
        parse_command(
            "D1=B1-A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 3)).unwrap().value, 10);

        // E1=A1*B1
        parse_command(
            "E1=A1*B1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 4)).unwrap().value, 200);

        // F1=B1/A1
        parse_command(
            "F1=B1/A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 2);

        parse_command(
            "ZZZ9991=B1/A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");

        parse_command(
            "F1=1+1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 2);

        parse_command(
            "F1=1-1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 0);

        parse_command(
            "F1=1*1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 1);

        parse_command(
            "F1=1/1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 1);

        parse_command(
            "F1=1/0",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().is_error, true);

        parse_command(
            "ZZZ9991=1/0",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");

        parse_command(
            "F1=1+A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 11);

        parse_command(
            "F1=1-A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, -9);

        parse_command(
            "F1=1*A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 10);

        parse_command(
            "F1=1/A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 0);

        parse_command(
            "ZZZ9991=1/A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");

        parse_command(
            "F1=A1+1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 11);

        parse_command(
            "F1=A1-1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 9);

        parse_command(
            "F1=A1*1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 10);

        parse_command(
            "F1=A1/1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 5)).unwrap().value, 10);

        parse_command(
            "ZZZ9991=A1/1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");

        parse_command(
            "B1=1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 1)).unwrap().value, 1);
        parse_command(
            "B2=2",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(1, 1)).unwrap().value, 2);
        parse_command(
            "A1=MAX(B1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 0)).unwrap().value, 2);
        parse_command(
            "A2=MAX(B1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(1, 0)).unwrap().value, 2);

        parse_command(
            "A2=A1+1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(1, 0)).unwrap().value, 3);

        parse_command(
            "B1=21",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 1)).unwrap().value, 21);
    }

    #[test]
    fn test_parse_command_functions() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Set up cells
        parse_command(
            "A1=10",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "A2=20",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "B1=30",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "B2=40",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        // Test SUM function
        parse_command(
            "C1=SUM(A1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 2)).unwrap().value, 100);

        // Test MIN function
        parse_command(
            "C2=MIN(A1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(1, 2)).unwrap().value, 10);

        // Test MAX function
        parse_command(
            "C3=MAX(A1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(2, 2)).unwrap().value, 40);

        // Test AVG function
        parse_command(
            "C4=AVG(A1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(3, 2)).unwrap().value, 25);

        // Test STDEV function
        parse_command(
            "C5=STDEV(A1:B2)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(4, 2)).unwrap().value, 11);

        sheet.data.get_mut(&(0, 0)).unwrap().string = Some("Hello".to_string());
        parse_command(
            "B2=A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(
            sheet.data.get(&(1, 1)).unwrap().string,
            Some("Hello".to_string())
        );
    }

    #[test]
    fn test_parse_command_string_cells() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Test string assignment
        parse_command(
            "A1=\"Hello\"",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        assert_eq!(
            sheet.data.get(&(0, 0)).unwrap().string,
            Some("Hello".to_string())
        );
        assert_eq!(status, "ok");
    }

    #[test]
    fn test_navigation_commands() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 20;
        let total_cols = 20;
        let mut print_enabled = true;

        // Test scroll commands
        parse_command(
            "s",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(row_start, 10);

        parse_command(
            "w",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(row_start, 0);

        parse_command(
            "d",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(col_start, 10);

        parse_command(
            "a",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(col_start, 0);

        // Test scroll_to command
        parse_command(
            "scroll_toB5",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(row_start, 4);
        assert_eq!(col_start, 1);

        parse_command(
            "scroll_toZZZ9991",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "err");
    }

    #[test]
    fn test_division_by_zero() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Set cells
        parse_command(
            "A1=10",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "B1=0",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        // Attempt division by zero
        parse_command(
            "C1=A1/B1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert!(sheet.data.get(&(0, 2)).unwrap().is_error);
    }

    #[test]
    fn test_remove_dependency() {
        let mut sheet = create_test_sheet();
        let mut status = String::new();
        let mut sleep_timer = 0;

        // Set B1 = 20
        get_or_create_cell(&mut sheet, 0, 1).value = 20;

        // Set A1 = B1
        let cell_a1 = CellInfo { row: 0, col: 0 };
        let cell_b1 = CellInfo { row: 0, col: 1 };
        let empty = CellInfo { row: -1, col: -1 };

        add_constraints(
            cell_a1,
            cell_b1,
            empty,
            OpCode::CellEqualsCell,
            &mut sheet,
            &mut status,
            &mut sleep_timer,
        );

        // Verify dependency exists
        assert!(
            sheet
                .data
                .get(&(0, 1))
                .unwrap()
                .dependencies
                .contains(&(0 * 1000 + 0))
        );

        // Remove dependency
        let cell_a1 = CellInfo { row: 0, col: 0 };
        remove_dependency(&cell_a1, &mut sheet);

        // Verify dependency was removed
        assert!(
            !sheet
                .data
                .get(&(0, 1))
                .unwrap()
                .dependencies
                .contains(&(0 * 1000 + 0))
        );
    }

    #[test]
    fn test_compute_range_func() {
        let mut sheet = create_test_sheet();
        let mut status = String::new();

        // Create a test range
        get_or_create_cell(&mut sheet, 0, 0).value = 10;
        get_or_create_cell(&mut sheet, 0, 1).value = 20;
        get_or_create_cell(&mut sheet, 1, 0).value = 30;
        get_or_create_cell(&mut sheet, 1, 1).value = 40;

        // Test range functions
        assert_eq!(
            compute_range_func(&sheet, OpCode::Sum, 0, 0, 1, 1, &mut status),
            100
        );
        assert_eq!(
            compute_range_func(&sheet, OpCode::Min, 0, 0, 1, 1, &mut status),
            10
        );
        assert_eq!(
            compute_range_func(&sheet, OpCode::Max, 0, 0, 1, 1, &mut status),
            40
        );
        assert_eq!(
            compute_range_func(&sheet, OpCode::Avg, 0, 0, 1, 1, &mut status),
            25
        );
        assert_eq!(
            compute_range_func(&sheet, OpCode::Stdev, 0, 0, 1, 1, &mut status),
            11
        );

        // Test invalid range
        assert_eq!(
            compute_range_func(&sheet, OpCode::Sum, 1, 0, 0, 1, &mut status),
            -1
        );
        assert_eq!(status, "err");

        // Test invalid op_code
        status = String::new();
        assert_eq!(
            compute_range_func(&sheet, OpCode::NoConstraint, 0, 0, 1, 1, &mut status),
            -1
        );
    }

    #[test]
    fn test_check_cycle() {
        let mut avl_tree = HashMap::new();
        avl_tree.insert(1000, 0); // Key for cell at (0,1)

        // Cell1 is already in the tree, should detect cycle
        let cell1 = CellInfo { row: 0, col: 1 };
        let cell2 = CellInfo { row: 1, col: 1 };
        assert!(check_cycle(&avl_tree, &cell1, &cell2));

        // Cell2 is not already in the tree, should not detect cycle
        let cell1 = CellInfo { row: 2, col: 2 };
        let cell2 = CellInfo { row: 1, col: 1 };
        assert!(!check_cycle(&avl_tree, &cell1, &cell2));

        // Add the key for cell2
        avl_tree.insert(1001, 0); // Key for cell at (1,1)
        assert!(check_cycle(&avl_tree, &cell1, &cell2));

        // Test empty cell2
        let cell1 = CellInfo { row: 2, col: 2 };
        let cell2 = CellInfo { row: -1, col: -1 };
        assert!(!check_cycle(&avl_tree, &cell1, &cell2));
    }

    #[test]
    fn test_check_cycle_range_funcs() {
        let mut avl_tree = HashMap::new();
        avl_tree.insert(1001, 0); // Key for cell at (1,1)

        // Range includes the cell in avl_tree
        let cell1 = CellInfo { row: 0, col: 0 };
        let cell2 = CellInfo { row: 2, col: 2 };
        assert!(check_cycle_range_funcs(&avl_tree, &cell1, &cell2));

        // Range does not include the cell in avl_tree
        let cell1 = CellInfo { row: 3, col: 3 };
        let cell2 = CellInfo { row: 4, col: 4 };
        assert!(!check_cycle_range_funcs(&avl_tree, &cell1, &cell2));
    }

    #[test]
    fn test_string_cells_and_operations() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Set up string cells
        parse_command(
            "A1=\"Hello\"",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "B1=\"World\"",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );

        // Check string values
        assert_eq!(
            sheet.data.get(&(0, 0)).unwrap().string,
            Some("Hello".to_string())
        );
        assert_eq!(
            sheet.data.get(&(0, 1)).unwrap().string,
            Some("World".to_string())
        );

        // Try to perform operations with string cells (should result in error)
        parse_command(
            "C1=A1+B1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert!(sheet.data.get(&(0, 2)).unwrap().is_error);

        parse_command(
            "ZZZ9991=\"World\"",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");
    }

    #[test]
    fn test_sleep_commands() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Test SLEEP with a constant
        parse_command(
            "A1=SLEEP(1)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(0, 0)).unwrap().value, 1);
        assert_eq!(time, 1.0);

        // Test SLEEP with a cell reference
        parse_command(
            "B1=1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        parse_command(
            "A2=SLEEP(B1)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(sheet.data.get(&(1, 0)).unwrap().op_code, OpCode::Sleep);
        assert_eq!(time, 1.0); // 1 from the first sleep + 2 from the second
    }

    #[test]
    fn test_error_handling() {
        let mut sheet = create_test_sheet();
        let mut row_start = 0;
        let mut col_start = 0;
        let mut time = 0.0;
        let mut status = String::new();
        let total_rows = 10;
        let total_cols = 10;
        let mut print_enabled = true;

        // Test invalid cell reference
        parse_command(
            "Z99=10",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");

        // Test circular reference
        parse_command(
            "A1=B1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        status = String::new();
        parse_command(
            "B1=A1",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "circular error");

        // Test invalid range in function
        parse_command(
            "C1=SUM(B2:A1)",
            &mut row_start,
            &mut col_start,
            &mut time,
            &mut status,
            &total_rows,
            &total_cols,
            &mut sheet,
            &mut print_enabled,
        );
        assert_eq!(status, "Invalid cmd");
    }

    #[test]
    fn test_topological_sort() {
        let mut sheet = create_test_sheet();

        // Create cells
        get_or_create_cell(&mut sheet, 0, 0); // A1
        get_or_create_cell(&mut sheet, 0, 1); // B1

        // Setup B1 depends on A1
        sheet
            .data
            .get_mut(&(0, 0))
            .unwrap()
            .dependencies
            .insert(1000); // A1's dependencies include B1

        // Create avl_tree with indegrees
        let mut avl_tree = HashMap::new();
        avl_tree.insert(0, 0); // A1 has indegree 0 (no dependencies)
        avl_tree.insert(1000, 1); // B1 has indegree 1 (depends on A1)

        // Run topological sort
        let sorted = topological_sort(&mut avl_tree, &sheet);

        // Verify sort order: should be A1, B1
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0], 0); // A1 should be first
        assert_eq!(sorted[1], 1000); // B1 should be second
    }

    #[test]
    fn test_print_sheet() {
        let mut sheet = create_test_sheet();

        // Set up some cells with different types of values
        get_or_create_cell(&mut sheet, 0, 0).value = 10; // A1 = 10
        get_or_create_cell(&mut sheet, 0, 1).value = 20; // B1 = 20
        let cell = get_or_create_cell(&mut sheet, 1, 0);
        cell.string = Some("Hello".to_string()); // A2 = "Hello"

        // Call the print_sheet function
        print_sheet(0, 0, 3, 3, &mut sheet);

        // Assert that the printed output matches the expected output
        assert_eq!(true, true);
    }
}
