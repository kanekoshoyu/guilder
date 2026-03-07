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

class Orderbook:
	"""order book, with asks and bids (key: price, value: volume)"""
	def __init__(self, asks: dict[float, float], bids: dict[float, float]):
		self.asks = asks
		self.bids = bids

class L2Update:
	"""single L2 orderbook price level update"""
	def __init__(self, symbol: str, price: float, volume: float, side: Side):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side

class Trade:
	"""market trade/fill event"""
	def __init__(self, symbol: str, price: float, volume: float, side: Side, timestamp: int):
		self.symbol = symbol
		self.price = price
		self.volume = volume
		self.side = side
		self.timestamp = timestamp

class TestServer(ABC):
	"""test server network connection"""
	@abstractmethod
	def ping(self) -> bool:
		"""test ping"""
		pass

	@abstractmethod
	def get_server_time(self) -> int:
		"""get server local time"""
		pass


class GetMarketData(ABC):
	"""get market data such as symbol, price and volume"""
	@abstractmethod
	def get_symbol(self) -> list[str]:
		"""get symbol, such as BTCUSD"""
		pass

	@abstractmethod
	def get_price(self, symbol: str) -> float:
		"""get mid-price of a symbol (e.g. BTCUSD -> 67000.0)"""
		pass

	@abstractmethod
	def get_orderbook(self, symbol: str) -> Orderbook:
		"""get orderbook"""
		pass


class ManageOrder(ABC):
	"""place, change, cancel order"""
	@abstractmethod
	def place_order(self, symbol: str, price: int, volume: int) -> int:
		"""place order, return cloid"""
		pass

	@abstractmethod
	def change_order_by_cloid(self, cloid: int, price: int, volume: int) -> int:
		"""change order"""
		pass

	@abstractmethod
	def cancel_order(self, cloid: int) -> int:
		"""cancel order by cloid"""
		pass

	@abstractmethod
	def cancel_all_order(self) -> bool:
		"""cancel all order regardless of cloid/symbol"""
		pass


class SubscribeMarketData(ABC):
	"""subscribe to streaming market data"""
	@abstractmethod
	async def subscribe_l2_update(self, symbol: str) -> AsyncIterator[L2Update]:
		"""subscribe to L2 orderbook updates for a symbol"""
		pass

	@abstractmethod
	async def subscribe_fill(self, symbol: str) -> AsyncIterator[Trade]:
		"""subscribe to market fill events for a symbol"""
		pass


