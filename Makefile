service_active:=$(shell systemctl is-active pwm-better-fan-control.service)

compile:
	if [ -d vendor ]; \
	then \
		cargo build --release --frozen; \
	else \
		cargo build --release; \
	fi

install-bin: compile
	chmod +x target/release/pwm-better-fan-control
ifeq ($(service_active) , active)
	sudo service pwm-better-fan-control stop
endif
	sudo cp target/release/pwm-better-fan-control /usr/local/bin/
ifeq ($(service_active) , active)
	sudo service pwm-better-fan-control start
endif

install-service:install-bin
	sudo cp pwm-better-fan-control.service /etc/systemd/system/
	sudo systemctl enable pwm-better-fan-control.service

all launch-service:install-service
	sudo service pwm-better-fan-control start
	
clean:
	rm -rf target

uninstall:
	sudo service pwm-better-fan-control stop
	sudo systemctl disable pwm-better-fan-control.service
	sudo rm /etc/systemd/system/pwm-better-fan-control.service
	sudo rm /usr/local/bin/pwm-better-fan-control
