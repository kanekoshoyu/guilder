from abc import ABC, abstractmethod
from enum import Enum
from typing import Iterator, AsyncIterator

class Status(Enum):
	Success = 1
	InProgress = 2
	Completed = 3
	Failed = 4

class Side(Enum):
	"""orderbook side"""
	Bid = 1
	Ask = 2

class OrderSide(Enum):
	"""direction of an order"""
	Buy = 1
	Sell = 2

class OrderStatus(Enum):
	"""lifecycle state of an order"""
	Placed = 1
	PartiallyFilled = 2
	Filled = 3
	Cancelled = 4

class MarketType(Enum):
	"""type of market"""
	Spot = 1
	Future = 2
	Perpetual = 3

class OrderType(Enum):
	"""order execution type"""
	Market = 1
	Limit = 2

class TimeInForce(Enum):
	"""how long an order remains active"""
	Gtc = 1
	Ioc = 2
	Fok = 3

class VolumeDenomination(Enum):
	"""which currency the volume is expressed in"""
	Base = 1
	Quote = 2

class AssetClass(Enum):
	"""broad class of asset"""
	Crypto = 1
	Stablecoin = 2
	Fiat = 3

class L2Update:
	"""single L2 orderbook price level update"""
	def __init__(self, symbol: str, price: str, volume: str, side: Side, sequence: int):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side
		self.sequence = sequence

class Liquidation:
	"""forced liquidation event"""
	def __init__(self, symbol: str, side: OrderSide, liquidated_user: str, notional_position: str, account_value: str):
		self.symbol = symbol
		self.side = side
		self.liquidated_user = liquidated_user
		self.notional_position = notional_position
		self.account_value = account_value

class AssetContext:
	"""snapshot of market metrics"""
	def __init__(self, symbol: str, open_interest: str, funding_rate: str, mark_price: str, day_volume: str, mid_price: Option<Decimal>, oracle_price: Option<Decimal>, premium: Option<Decimal>, prev_day_price: Option<Decimal>, sz_decimals: int):
		self.symbol = symbol
		self.open_interest = open_interest
		self.funding_rate = funding_rate
		self.mark_price = mark_price
		self.day_volume = day_volume
		self.mid_price = mid_price
		self.oracle_price = oracle_price
		self.premium = premium
		self.prev_day_price = prev_day_price
		self.sz_decimals = sz_decimals

class PredictedFunding:
	"""predicted funding rate for a symbol at a venue"""
	def __init__(self, symbol: str, venue: str, funding_rate: str, next_funding_time_ms: int):
		self.symbol = symbol
		self.venue = venue
		self.funding_rate = funding_rate
		self.next_funding_time_ms = next_funding_time_ms

class Fill:
	"""market trade event"""
	def __init__(self, symbol: str, price: str, volume: str, side: OrderSide, timestamp_ms: int, trade_id: int):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side
		self.timestamp_ms = timestamp_ms
		self.trade_id = trade_id

class Position:
	"""open trading position"""
	def __init__(self, symbol: str, side: OrderSide, size: str, entry_price: str):
		self.symbol = symbol
		self.side = side
		self.size = size
		self.entry_price = entry_price

class OpenOrder:
	"""resting order"""
	def __init__(self, order_id: int, symbol: str, side: OrderSide, price: str, quantity: str, filled_quantity: str):
		self.order_id = order_id
		self.symbol = symbol
		self.side = side
		self.price = price
		self.quantity = quantity
		self.filled_quantity = filled_quantity

class OrderPlacement:
	"""order placement response"""
	def __init__(self, order_id: int, symbol: str, side: OrderSide, price: str, quantity: str, timestamp_ms: int, cloid: Option<String>):
		self.order_id = order_id
		self.symbol = symbol
		self.side = side
		self.price = price
		self.quantity = quantity
		self.timestamp_ms = timestamp_ms
		self.cloid = cloid

class UserFill:
	"""execution of the user's own order"""
	def __init__(self, order_id: int, symbol: str, side: OrderSide, price: str, quantity: str, fee_usd: str, timestamp_ms: int, cloid: Option<String>):
		self.order_id = order_id
		self.symbol = symbol
		self.side = side
		self.price = price
		self.quantity = quantity
		self.fee_usd = fee_usd
		self.timestamp_ms = timestamp_ms
		self.cloid = cloid

class OrderUpdate:
	"""order lifecycle update"""
	def __init__(self, order_id: int, symbol: str, status: OrderStatus, side: Option<OrderSide>, price: Option<Decimal>, quantity: Option<Decimal>, remaining_quantity: Option<Decimal>, timestamp_ms: int, cloid: Option<String>):
		self.order_id = order_id
		self.symbol = symbol
		self.status = status
		self.side = side
		self.price = price
		self.quantity = quantity
		self.remaining_quantity = remaining_quantity
		self.timestamp_ms = timestamp_ms
		self.cloid = cloid

class FundingPayment:
	"""funding payment applied to a position"""
	def __init__(self, symbol: str, amount_usd: str, timestamp_ms: int):
		self.symbol = symbol
		self.amount_usd = amount_usd
		self.timestamp_ms = timestamp_ms

class Deposit:
	"""deposit event"""
	def __init__(self, asset: str, amount_usd: str, timestamp_ms: int):
		self.asset = asset
		self.amount_usd = amount_usd
		self.timestamp_ms = timestamp_ms

class Withdrawal:
	"""withdrawal event"""
	def __init__(self, asset: str, amount_usd: str, timestamp_ms: int):
		self.asset = asset
		self.amount_usd = amount_usd
		self.timestamp_ms = timestamp_ms

class Balance:
	"""account balance for an asset"""
	def __init__(self, coin: str, total: str, available: str, locked: str):
		self.coin = coin
		self.total = total
		self.available = available
		self.locked = locked

class UserRateLimit:
	"""user's address-level API rate limit budget from Hyperliquid's userRateLimit endpoint"""
	def __init__(self, cumulative_volume: str, requests_used: int, requests_cap: int, requests_surplus: int):
		self.cumulative_volume = cumulative_volume
		self.requests_used = requests_used
		self.requests_cap = requests_cap
		self.requests_surplus = requests_surplus

class TestServer(ABC):
	"""test server network connection"""
	@abstractmethod
	async def ping(self) -> Result<bool, String>:
		"""test ping"""
		pass

	@abstractmethod
	async def get_server_time(self) -> Result<i64, String>:
		"""get server local time"""
		pass


class GetMarketData(ABC):
	"""get market data such as symbol, price and volume"""
	@abstractmethod
	async def get_symbol(self) -> Result<Vec<String>, String>:
		"""get symbol, such as BTCUSD"""
		pass

	@abstractmethod
	async def get_price(self, symbol: str) -> Result<Decimal, String>:
		"""get mid-price of a symbol (e.g. BTCUSD -> 67000.0)"""
		pass

	@abstractmethod
	async def get_open_interest(self, symbol: str) -> Result<Decimal, String>:
		"""get current open interest for a symbol"""
		pass

	@abstractmethod
	async def get_asset_context(self, symbol: str) -> Result<AssetContext, String>:
		"""get snapshot of market metrics (OI, funding rate, mark price, 24h volume)"""
		pass

	@abstractmethod
	async def get_all_asset_contexts(self) -> Result<Vec<AssetContext>, String>:
		"""get all asset context snapshots in one call (prefer over repeated get_asset_context)"""
		pass

	@abstractmethod
	async def get_sz_decimals(self, symbol: str) -> Result<i32, String>:
		"""get the number of decimal places for order size for a symbol (static metadata)"""
		pass

	@abstractmethod
	async def get_all_sz_decimals(self) -> Result<HashMap<String, i32>, String>:
		"""get sz_decimals for all symbols in one call (prefer over repeated get_sz_decimals)"""
		pass

	@abstractmethod
	async def get_predicted_fundings(self) -> Result<Vec<PredictedFunding>, String>:
		"""get predicted funding rates for all symbols across all venues"""
		pass

	@abstractmethod
	async def get_l2_orderbook(self, symbol: str) -> Result<Vec<L2Update>, String>:
		"""get full L2 orderbook snapshot for a symbol (for initialization)"""
		pass


class ManageOrder(ABC):
	"""place, change, cancel order"""
	@abstractmethod
	async def place_order(self, symbol: str, side: OrderSide, price: str, volume: str, order_type: OrderType, time_in_force: TimeInForce, cloid: Option<String>) -> Result<OrderPlacement, String>:
		"""place order with optional client order ID for end-to-end tracking"""
		pass

	@abstractmethod
	async def change_order_by_cloid(self, cloid: int, price: str, volume: str) -> Result<i64, String>:
		"""change order"""
		pass

	@abstractmethod
	async def cancel_order_by_cloid(self, cloid: str) -> Result<(), String>:
		"""cancel order by cloid"""
		pass

	@abstractmethod
	async def cancel_all_order(self) -> Result<bool, String>:
		"""cancel all order regardless of cloid/symbol"""
		pass


class SubscribeMarketData(ABC):
	"""subscribe to streaming market data"""
	@abstractmethod
	async def subscribe_l2_update(self, symbol: str) -> AsyncIterator[Result<L2Update, String>]:
		"""subscribe to L2 orderbook updates for a symbol"""
		pass

	@abstractmethod
	async def subscribe_fill(self, symbol: str) -> AsyncIterator[Result<Fill, String>]:
		"""subscribe to market fill events for a symbol"""
		pass

	@abstractmethod
	async def subscribe_asset_context(self, symbol: str) -> AsyncIterator[Result<AssetContext, String>]:
		"""subscribe to asset context updates (OI, funding rate, mark price, 24h volume)"""
		pass

	@abstractmethod
	async def subscribe_liquidation(self, user: str) -> AsyncIterator[Result<Liquidation, String>]:
		"""subscribe to liquidation events for a user address"""
		pass


class GetAccountSnapshot(ABC):
	"""query authenticated account snapshot"""
	@abstractmethod
	async def get_positions(self) -> Result<Vec<Position>, String>:
		"""get current open positions"""
		pass

	@abstractmethod
	async def get_open_orders(self) -> Result<Vec<OpenOrder>, String>:
		"""get currently resting orders"""
		pass

	@abstractmethod
	async def get_collateral(self) -> Result<Decimal, String>:
		"""get available account collateral"""
		pass

	@abstractmethod
	async def get_spot_balance(self) -> Result<Vec<Balance>, String>:
		"""get all spot wallet balances"""
		pass

	@abstractmethod
	async def get_collateral_balance(self, asset: str) -> Result<Balance, String>:
		"""get clearing house collateral balance (typically USDC only)"""
		pass

	@abstractmethod
	async def get_user_rate_limit(self) -> Result<UserRateLimit, String>:
		"""get user's address-level API rate limit budget (remaining requests, cumulative volume, cap)"""
		pass


class SubscribeUserEvents(ABC):
	"""subscribe to authenticated user account events"""
	@abstractmethod
	async def subscribe_user_fills(self) -> AsyncIterator[Result<UserFill, String>]:
		"""stream executions of the user's own orders"""
		pass

	@abstractmethod
	async def subscribe_order_updates(self) -> AsyncIterator[Result<OrderUpdate, String>]:
		"""stream order lifecycle updates"""
		pass

	@abstractmethod
	async def subscribe_funding_payments(self) -> AsyncIterator[Result<FundingPayment, String>]:
		"""stream funding payments applied to positions"""
		pass

	@abstractmethod
	async def subscribe_deposits(self) -> AsyncIterator[Result<Deposit, String>]:
		"""stream account deposit events"""
		pass

	@abstractmethod
	async def subscribe_withdrawals(self) -> AsyncIterator[Result<Withdrawal, String>]:
		"""stream account withdrawal events"""
		pass

	@abstractmethod
	async def subscribe_spot_balance(self) -> AsyncIterator[Result<Vec<Balance>, String>]:
		"""subscribe to spot wallet balance updates for the registered user address (requires authentication)"""
		pass

	@abstractmethod
	async def subscribe_spot_balance_with_address(self, address: str) -> AsyncIterator[Result<Vec<Balance>, String>]:
		"""subscribe to spot wallet balance updates for a specific address"""
		pass


