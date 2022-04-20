use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::fs::File;
use std::io::prelude::*;

mod bigint;

//pub const MODEL_FEE_NUMERATOR: u64 = 10000000;
//pub const MODEL_FEE_DENOMINATOR: u64 = 10000000000;

//const DEFAULT_POOL_TOKENS: u128 = 0;
//const DEFAULT_TARGET_PRICE: u128 = 1000000000000000000;
//const DEFAULT_RATES: [u128;3] = [1000000, 1000000000000000000, 1000000000000000000];
//const DEFAULT_TRADE_FEE: u128 = 4000000;
//const DEFAULT_WITHDRAW_FEE: u128 = 0;
const FILE_NAME: &str = "simulation.py";
const FILE_PATH: &str = "../sim/simulation.py";
const MODULE_NAME: &str = "simulation";

pub struct Model {
    py_src: String,
    pub amp_factor: u64,
    pub balances: Vec<u128>,
    pub n_coins: u8,
    pub target_prices: Vec<u128>,
    pub trade_fee: u128,
    pub withdraw_fee: u128,
    pub pool_tokens: u128,
}

impl Model {
    pub fn new(
        amp_factor: u64,
        balances: Vec<u128>,
        n_coins: u8,
        rates: Vec<u128>,
        trade_fee: u128,
        withdraw_fee: u128,
        tokens: u128,
    ) -> Model {
        let src_file = File::open(FILE_PATH);
        let mut src_file = match src_file {
            Ok(file) => file,
            Err(error) => {
                panic!("{:?}\n Please run `curl -L
            https://raw.githubusercontent.com/curvefi/curve-contract/master/tests/simulation.py > sim/simulation.py`", error)
            }
        };
        let mut src_content = String::new();
        let _ = src_file.read_to_string(&mut src_content);

        Self {
            py_src: src_content,
            amp_factor,
            balances,
            n_coins,
            target_prices: rates,
            trade_fee,
            withdraw_fee,
            pool_tokens: tokens,
        }
    }

    pub fn new_with_pool_tokens(
        amp_factor: u64,
        balances: Vec<u128>,
        n_coins: u8,
        rates: Vec<u128>,
        trade_fee: u128,
        withdraw_fee: u128,
        tokens: u128,
    ) -> Model {
        let src_file = File::open(FILE_PATH);
        let mut src_file = match src_file {
            Ok(file) => file,
            Err(error) => {
                panic!("{:?}\n Please run `curl -L
            https://raw.githubusercontent.com/curvefi/curve-contract/master/tests/simulation.py > sim/simulation.py`", error)
            }
        };
        let mut src_content = String::new();
        let _ = src_file.read_to_string(&mut src_content);

        Self {
            py_src: src_content,
            amp_factor,
            balances,
            n_coins,
            target_prices: rates,
            trade_fee,
            withdraw_fee,
            pool_tokens: tokens,
        }
    }

    pub fn sim_get_vp(&self) -> u128 {
        let gil = Python::acquire_gil();
        return self
            .call0(gil.python(), "get_virtual_price")
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_d(&self) -> bigint::U576 {
        let gil = Python::acquire_gil();
        let str_d = self.call0(gil.python(), "D").unwrap().to_string();
        return bigint::U576::from_dec_str(&str_d).unwrap();
    }

    pub fn sim_add_liq3(&self, deposit_amounts: [u128; 3]) -> u128 {
        let gil = Python::acquire_gil();
        return self
            .call1(
                gil.python(),
                "add_liq3",
                (deposit_amounts[0], deposit_amounts[1], deposit_amounts[2]),
            )
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_dy(&self, i: u128, j: u128, dx: u128) -> u128 {
        let gil = Python::acquire_gil();
        return self
            .call1(gil.python(), "dy", (i, j, dx))
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_exchange(&self, i: u8, j: u8, dx: u128) -> (u128, u128) {
        let gil = Python::acquire_gil();
        return self
            .call1(gil.python(), "exchange", (i, j, dx))
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_xp(&self) -> Vec<u128> {
        let gil = Python::acquire_gil();
        return self
            .call0(gil.python(), "xp")
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_y(&self, i: u8, j: u8, x: u128) -> bigint::U576 {
        let gil = Python::acquire_gil();
        let str_y = self
            .call1(gil.python(), "y", (i, j, x))
            .unwrap()
            .to_string();
        return bigint::U576::from_dec_str(&str_y).unwrap();
    }

    pub fn sim_y_d(&self, i: u8, str_d: String) -> bigint::U576 {
        let gil = Python::acquire_gil();
        let d: bigint::U576 = bigint::U576::from_dec_str(&str_d).unwrap();
        let str_y_d = self
            .call1(gil.python(), "y_D", (i, d.to_u128()))
            .unwrap()
            .to_string();
        return bigint::U576::from_dec_str(&str_y_d).unwrap();
    }

    pub fn sim_remove_liq3(&self, token_amount: u128, nonce: u8) -> (u128, u128, u128) {
        let gil = Python::acquire_gil();
        return self
            .call1(gil.python(), "remove_liq3", (token_amount, nonce))
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_remove_liquidity_imbalance(&self, amounts: Vec<u128>) -> u128 {
        println!("aaa {} {} {} \n", amounts[0], amounts[1], amounts[2]);
        let gil = Python::acquire_gil();
        return self
            .call1(
                gil.python(),
                "remove_liquidity_imbalance",
                PyTuple::new(gil.python(), amounts.to_vec()),
            )
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_remove_liq_imba3(&self, coin0: u128, coin1: u128, coin2: u128) -> u128 {
        let gil = Python::acquire_gil();
        return self
            .call1(gil.python(), "remove_liq_imba3", (coin0, coin1, coin2))
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    pub fn sim_calc_withdraw_one_coin(&self, token_amount: u128, i: u8) -> u128 {
        let gil = Python::acquire_gil();
        return self
            .call1(gil.python(), "calc_withdraw_one_coin", (token_amount, i))
            .unwrap()
            .extract(gil.python())
            .unwrap();
    }

    fn call0(&self, py: Python, method_name: &str) -> Result<PyObject, PyErr> {
        let sim = PyModule::from_code(py, &self.py_src, FILE_NAME, MODULE_NAME).unwrap();
        let model = sim
            .call1(
                "SnailSwap",
                (
                    self.amp_factor,
                    self.balances.to_vec(),
                    self.n_coins,
                    self.target_prices.to_vec(),
                    self.trade_fee,
                    self.withdraw_fee,
                    self.pool_tokens,
                ),
            )
            .unwrap()
            .to_object(py);
        let py_ret = model.as_ref(py).call_method0(method_name);
        self.extract_py_ret(py, py_ret)
    }

    fn call1(
        &self,
        py: Python,
        method_name: &str,
        args: impl IntoPy<Py<PyTuple>>,
    ) -> Result<PyObject, PyErr> {
        let sim = PyModule::from_code(py, &self.py_src, FILE_NAME, MODULE_NAME).unwrap();
        let model = sim
            .call1(
                "SnailSwap",
                (
                    self.amp_factor,
                    self.balances.to_vec(),
                    self.n_coins,
                    self.target_prices.to_vec(),
                    self.trade_fee,
                    self.withdraw_fee,
                    self.pool_tokens,
                ),
            )
            .unwrap()
            .to_object(py);
        let py_ret = model.as_ref(py).call_method1(method_name, args);
        self.extract_py_ret(py, py_ret)
    }

    fn extract_py_ret(&self, py: Python, ret: PyResult<&PyAny>) -> Result<PyObject, PyErr> {
        match ret {
            Ok(v) => v.extract(),
            Err(e) => {
                e.print_and_set_sys_last_vars(py);
                panic!("Python exeuction failed.")
            }
        }
    }

    pub fn print_src(&self) {
        println!("{}", self.py_src);
    }
}
