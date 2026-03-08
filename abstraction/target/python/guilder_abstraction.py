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
	"""broad class of an asset"""
	Crypto = 1
	Stablecoin = 2
	Fiat = 3

class L2Update:
	"""single L2 orderbook price level update"""
	def __init__(self, symbol: str, price: float, volume: float, side: Side, sequence: int):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side
		self.sequence = sequence

class Liquidation:
	"""forced liquidation event for a user"""
	def __init__(self, liquidated_user: str, notional_position: float, account_value: float):
		self.liquidated_user = liquidated_user
		self.notional_position = notional_position
		self.account_value = account_value

class AssetContext:
	"""snapshot of key market metrics for an asset"""
	def __init__(self, symbol: str, open_interest: float, funding_rate: float, mark_price: float, day_volume: float):
		self.symbol = symbol
		self.open_interest = open_interest
		self.funding_rate = funding_rate
		self.mark_price = mark_price
		self.day_volume = day_volume

class Fill:
	"""market trade/fill event"""
	def __init__(self, symbol: str, price: float, volume: float, side: OrderSide, timestamp_ms: int, trade_id: int):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side
		self.timestamp_ms = timestamp_ms
		self.trade_id = trade_id

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
	async def get_price(self, symbol: str) -> Result<f64, String>:
		"""get mid-price of a symbol (e.g. BTCUSD -> 67000.0)"""
		pass

	@abstractmethod
	async def get_open_interest(self, symbol: str) -> Result<f64, String>:
		"""get current open interest for a symbol"""
		pass


class ManageOrder(ABC):
	"""place, change, cancel order"""
	@abstractmethod
	async def place_order(self, symbol: str, price: float, volume: float) -> Result<i64, String>:
		"""place order, return cloid"""
		pass

	@abstractmethod
	async def change_order_by_cloid(self, cloid: int, price: float, volume: float) -> Result<i64, String>:
		"""change order"""
		pass

	@abstractmethod
	async def cancel_order(self, cloid: int) -> Result<i64, String>:
		"""cancel order by cloid"""
		pass

	@abstractmethod
	async def cancel_all_order(self) -> Result<bool, String>:
		"""cancel all order regardless of cloid/symbol"""
		pass


class SubscribeMarketData(ABC):
	"""subscribe to streaming market data"""
	@abstractmethod
	async def subscribe_l2_update(self, symbol: str) -> AsyncIterator[L2Update]:
		"""subscribe to L2 orderbook updates for a symbol"""
		pass

	@abstractmethod
	async def subscribe_fill(self, symbol: str) -> AsyncIterator[Fill]:
		"""subscribe to market fill events for a symbol"""
		pass

	@abstractmethod
	async def subscribe_asset_context(self, symbol: str) -> AsyncIterator[AssetContext]:
		"""subscribe to asset context updates (OI, funding rate, mark price, 24h volume)"""
		pass

	@abstractmethod
	async def subscribe_liquidation(self, user: str) -> AsyncIterator[Liquidation]:
		"""subscribe to liquidation events for a user address"""
		pass


