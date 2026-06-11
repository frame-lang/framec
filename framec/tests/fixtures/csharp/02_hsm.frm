@@system MiniHsm {
    interface:
        wake()
        sleep()
        signal()

    machine:
        $Live {
            wake() { }
            sleep() { }
            signal() { }
        }

        $Awake => $Live {
            $>() { @@:self.awakes = @@:self.awakes + 1 }
            signal() {
                @@:self.last = 1
                => $^
            }
        }

        $Asleep => $Live {
            $>() { @@:self.sleeps = @@:self.sleeps + 1 }
            signal() {
                @@:self.last = 2
                => $^
            }
            wake() { -> $Awake }
        }

    domain:
        awakes: int = 0
        sleeps: int = 0
        last: int = 0
}
