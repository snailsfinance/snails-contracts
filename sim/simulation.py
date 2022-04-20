

PRECISION = 10 ** 24
FEE_DENOMINATOR = 10 ** 10

class SnailSwap:
    def __init__(self, A, D, n, p=None, trade_fee=None, withdraw_fee=None, tokens=None):
        """
        A: Amplification coefficient
        D: Total deposit. vector=balances; scala = [D/n] *n
        n: number of currencies
        p: precision target prices
        tokens: total_supply
        """
        self.A = A  # actually A * n ** (n - 1) because it's an invariant
        self.n = n
        
        if trade_fee:
            self.fee = trade_fee
        else:
            self.fee = 4000000

        if withdraw_fee:
            self.withdraw_fee = withdraw_fee
        else:
            self.withdraw_fee = 0
 
        #default rates = [1e24,1e24,1e24]
        #3pool = [1e6,1e18,1e18]
        if p:
            self.p = p
        else:
            self.p = [10 ** 24] * n
            
        #vector balances = D
        #scala  balances = [D/n] * n
        if isinstance(D, list):
            self.x = D
        else:
            self.x = [D // n * 10 ** 24 // _p for _p in self.p]

        if tokens:
            self.tokens = tokens
        else:
            self.tokens = 0

    def xp(self):
        return [x * p for x, p in zip(self.x, self.p)]

    def D(self):
        """
        D invariant calculation in non-overflowing integer operations
        iteratively

        A * sum(x_i) * n**n + D = A * D * n**n + D**(n+1) / (n**n * prod(x_i))

        Converging solution:
        D[j+1] = (A * n**n * sum(x_i) - D[j]**(n+1) / (n**n prod(x_i))) / (A * n**n - 1)
        """
        Dprev = 0
        xp = self.xp()
        S = sum(xp)
        D = S
        Ann = self.A * self.n
        while abs(D - Dprev) > 1:
            D_P = D
            for x in xp:
                D_P = D_P * D // (self.n * x)
            Dprev = D
            D = (Ann * S + D_P * self.n) * D // ((Ann - 1) * D + (self.n + 1) * D_P)

        return D

    def get_virtual_price(self):
        d = self.D()
        #print(d)
        return PRECISION * d // self.tokens
    
    def add_liq(self,deposit_amounts):
        _fee = self.fee * self.n // (4 * (self.n - 1))
        
        old_balances = self.x
        new_balances = self.x[:]
        
        d0 = 0
        if self.tokens>0:
            d0 = self.D()

        #print(new_balances)
        for i in range(self.n):
            if self.tokens == 0:
                assert(deposit_amounts[i] > 0)
            new_balances[i] += deposit_amounts[i]
        
        self.x = new_balances
        d1 = self.D()

        d2 = d1
        fees = [0] * self.n
        if self.tokens > 0:
            for i in range(self.n):
                ideal_balance = d1 * old_balances[i] // d0
                difference = abs(ideal_balance - new_balances[i])
                fees[i] = _fee * difference // FEE_DENOMINATOR
                new_balances[i] -= fees[i]
                #print(i,ideal_balance,new_balances[i])
        self.x = new_balances
        d2 = self.D()
        
        if self.tokens == 0:
            mint_amount = d1
        else:
            mint_amount = self.tokens * (d2-d0) // d0
        return mint_amount
    
    def add_liq3(self,m0,m1,m2):
        ms = [m0,m1,m2]
        return self.add_liq(ms)
        
    def y(self, i, j, x):
        """
        Calculate x[j] if one makes x[i] = x

        Done by solving quadratic equation iteratively.
        x_1**2 + x1 * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n+1)/(n ** (2 * n) * prod' * A)
        x_1**2 + b*x_1 = c

        x_1 = (x_1**2 + c) / (2*x_1 + b)
        """
        D = self.D()
        xx = self.xp()
        xx[i] = x  # x is quantity of underlying asset brought to 1e24 precision
        xx = [xx[k] for k in range(self.n) if k != j]
        Ann = self.A * self.n
        c = D
        for y in xx:
            c = c * D // (y * self.n)
        c = c * D // (self.n * Ann)
        b = sum(xx) + D // Ann - D
        y_prev = 0
        y = D
        while abs(y - y_prev) > 1:
            y_prev = y
            y = (y ** 2 + c) // (2 * y + b)
        return y  # the result is in underlying units too

    def y_D(self, i, _D):
        """
        Calculate x[j] if one makes x[i] = x

        Done by solving quadratic equation iteratively.
        x_1**2 + x1 * (sum' - (A*n**n - 1) * D / (A * n**n)) = D ** (n+1)/(n ** (2 * n) * prod' * A)
        x_1**2 + b*x_1 = c

        x_1 = (x_1**2 + c) / (2*x_1 + b)
        """
        xx = self.xp()
        xx = [xx[k] for k in range(self.n) if k != i]
        S = sum(xx)
        Ann = self.A * self.n
        c = _D
        for y in xx:
            c = c * _D // (y * self.n)
        c = c * _D // (self.n * Ann)
        b = S + _D // Ann
        y_prev = 0
        y = _D
        while abs(y - y_prev) > 1:
            y_prev = y
            y = (y ** 2 + c) // (2 * y + b - _D)
        return y  # the result is in underlying units too

    ### precise to match with rust precision
    def y_D_precise(self, xp, i, _D):
        xx = xp
        xx = [xx[k] for k in range(self.n) if k != i]
        S = sum(xx)
        Ann = self.A * self.n
        c = _D
        for y in xx:
            c = c * _D // (y * self.n)
        c = c * _D // (self.n * Ann)
        b = S + _D // Ann
        y_prev = 0
        y = _D
        while abs(y - y_prev) > 1:
            y_prev = y
            y = (y ** 2 + c) // (2 * y + b - _D)
        return y  # the result is in underlying units too
    
    def dy(self, i, j, dx):
        # dx and dy are in underlying units
        xp = self.xp()
        return xp[j] - self.y(i, j, xp[i] + dx)

    def exchange(self, i, j, dx):
        xp = self.xp()
        x = xp[i] + dx * self.p[i]
        y = self.y(i, j, x)
        dy = xp[j] - y - 1
        fee = dy * self.fee // FEE_DENOMINATOR
        assert dy > 0
        return (dy - fee) // self.p[j] ,fee // self.p[j]

    def remove_liq(self, token_amount):
        receive_amounts = [0] * self.n
        for i in range(self.n):
            value = self.x[i] * token_amount // self.tokens
            withdraw_fee = value * self.withdraw_fee // FEE_DENOMINATOR
            receive_amounts[i] = value - withdraw_fee
        return receive_amounts

    def remove_liq3(self, token_amount, nonce):
        receive_amounts = self.remove_liq(token_amount)
        return receive_amounts[0], receive_amounts[1], receive_amounts[2]
    
    def remove_liquidity_imbalance(self, amounts):
        _fee = self.fee * self.n // (4 * (self.n - 1))

        old_balances = self.x
        new_balances = self.x[:]
        D0 = self.D()
        for i in range(self.n):
            new_balances[i] -= amounts[i]
        self.x = new_balances

        D1 = self.D()
        #self.x = old_balances
        fees = [0] * self.n
        for i in range(self.n):
            ideal_balance = D1 * old_balances[i] // D0
            difference = abs(ideal_balance - new_balances[i])
            fees[i] = _fee * difference // FEE_DENOMINATOR
            withdraw_fee = amounts[i] * self.withdraw_fee // FEE_DENOMINATOR
            new_balances[i] -= (fees[i] + withdraw_fee)
            
        self.x = new_balances
        D2 = self.D()

        token_amount = (D0 - D2) * self.tokens // D0
        token_amount += 1
        return token_amount

    def remove_liq_imba3(self, m0, m1, m2):
        amounts = [m0, m1, m2]
        return self.remove_liquidity_imbalance(amounts)
    
    def calc_withdraw_one_coin(self, token_amount, i):
        xp = self.xp()
        _fee = self.fee * self.n // (4 * (self.n - 1))

        D0 = self.D()
        D1 = D0 - token_amount * D0 // self.tokens
        new_y = self.y_D(i, D1)
        dy_0 = xp[i] - new_y
        
        xp_reduced = xp
        for j in range(self.n):
            dx_idea = 0
            if j == i:
                dx_idea = xp[j] * D1 // D0 - new_y
            else:
                dx_idea = xp[j] - xp[j] * D1 // D0
            
            xp_reduced[j] -= _fee * dx_idea // FEE_DENOMINATOR
            self.x[j] = xp_reduced[j] // self.p[j]
            
        #dy = xp_reduced[i] - self.y_D(i,D1)
        dy = xp_reduced[i] - self.y_D_precise(xp_reduced,i,D1)
        dy -= 1
        
        withdraw_fee = dy * self.withdraw_fee // FEE_DENOMINATOR     
        total_fee = dy_0 - dy + withdraw_fee
        #print('receive',dy - withdraw_fee,'total_fee',total_fee)
        return (dy - withdraw_fee) // self.p[i]



def test_initialize():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [20000000000000000000000, 30000000000, 40000000000]
    snails_model = SnailSwap(A, balances, n_coins, rates)
    print(snails_model.x)
    print(snails_model.xp())    
    
    
    D = 30000
    snails_model = SnailSwap(A, D, n_coins, rates)
    print(snails_model.x)
    print(snails_model.xp())
    
    
def test_get_d():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [20000000000000000000000, 30000000000, 40000000000]
    snails_model = SnailSwap(A, balances, n_coins, rates)
    print(snails_model.x)
    print(snails_model.xp())    
    
    ###test_get_d###
    print(snails_model.D())
    

def test_get_virtual_price():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [10000000000000000000000, 20000000000, 30000000000]
    total_supply = 30000 * PRECISION
    snails_model = SnailSwap(A, balances, n_coins, rates, total_supply)
    print(snails_model.x)
    print(snails_model.xp())    
    
    ###test_get_d###
    vp = snails_model.get_virtual_price()
    print('vp = %d\n'%(vp))   
    print('vp = %d\n'%(snails_model.D()/30000))  

def test_add_liq():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [100000000000000, 100, 100]
    deposit = [200000000000000, 300, 400]
    total_supply = 3 * 100000000000000 * 1000000
    #total_supply = 0
    snails_model = SnailSwap(A, balances, n_coins, rates, total_supply)
    #print(snails_model.x)
    #print(snails_model.xp())   
    
    ###test_add_liq###
    mint = snails_model.add_liq(deposit)
    print(mint)
   
   
def test_exchange():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [1000000000000000000, 2000000, 3000000]
    i=2
    j=1
    dx = 200000
    snails_model = SnailSwap(A, balances, n_coins, rates)
    
    
    ###test_add_liq###
    receive,fee = snails_model.exchange(i,j,dx)
    print(receive,fee)
  

def test_remove_liq():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [1000000000000000000, 2000000, 3000000]
    tt = 3 * 100000000000000 * 10000000000
    snails_model = SnailSwap(A, balances, n_coins, rates, 4000000, 4000000, tt)
     
    ###test_add_liq###
    receive_amounts = snails_model.remove_liq(tt / 3)
    print(receive_amounts)
   
   
def test_remove_liq_imba():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [10000000000000000000000, 10000000000, 10000000000]
    remove_amounts = [500000000000000000000, 600000000, 800000000]
    tt = 3 * 100000000000000 * 100000000000000
    snails_model = SnailSwap(A, balances, n_coins, rates, 4000000, 3000000, tt)
     
    ###test_add_liq###
    burn_lp = snails_model.remove_liquidity_imbalance(remove_amounts)
    print(burn_lp)

def test_remove_liq_one_coin():
    n_coins = 3
    rates = [10 ** 6, 10 ** 18, 10 ** 18]
    A = 2 * 360
    balances = [1000000000000000000, 1000000, 1000000]
    tt = 3 * 100000000000000 * 10000000000
    rmamt = 3 * 100000000000000 * 1000000000
    snails_model = SnailSwap(A, balances, n_coins, rates, 4000000, 0000000, tt)
     
    ###test_add_liq###
    receive_amt = snails_model.calc_withdraw_one_coin(rmamt,0)
    print(receive_amt)
             
if __name__ == '__main__':
    #test_initialize()
    test_get_d()
    #test_get_virtual_price()
    #test_add_liq()
    #test_exchange()
    #test_remove_liq()
    #test_remove_liq_imba()
    #test_remove_liq_one_coin()
    