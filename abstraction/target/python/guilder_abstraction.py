from abc import ABC, abstractmethod

"""test server network connection"""
class TestServer(ABC):
	"""test ping"""
	@abstractmethod
	def ping(self) -> bool:
		pass

	"""get server local time"""
	@abstractmethod
	def get_server_time(self) -> int:
		pass


"""get market data such as symbol, price and volume"""
class GetMarketData(ABC):
	"""get symbol, such as BTCUSD"""
	@abstractmethod
	def get_symbol(self) -> list[str]:
		pass

	"""get mid-price of a symbol (e.g. BTCUSD -> 67000.0)"""
	@abstractmethod
	def get_price(self, symbol: str) -> float:
		pass

	"""get orderbook"""
	@abstractmethod
	def get_orderbook(self, symbol: str) -> dict[float, float]:
		pass


"""place, change, cancel order"""
class ManageOrder(ABC):
	"""place order, return cloid"""
	@abstractmethod
	def place_order(self, symbol: str, price: int, volume: int) -> int:
		pass

	"""change order"""
	@abstractmethod
	def change_order_by_cloid(self, cloid: int, price: int, volume: int) -> int:
		pass

	"""cancel order by cloid"""
	@abstractmethod
	def cancel_order(self, cloid: int) -> int:
		pass

	"""cancel all order regardless of cloid/symbol"""
	@abstractmethod
	def cancel_all_order(self) -> bool:
		pass


