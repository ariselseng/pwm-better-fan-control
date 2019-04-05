# pwm-better-fan-control
A simple rust daemon to make my laptop produce less noise. Use at your own risk.

It has a staggered spin down process for less annoying noise. Also sets vendor fan mode back to auto at exit.
Needs system76-dkms at the moment. 

Got inspired by code here: https://github.com/pop-os/system76-power/blob/master/src/fan.rs