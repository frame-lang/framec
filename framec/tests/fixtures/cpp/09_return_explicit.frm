@@system ReturnExplicit {
    interface:
        decide(score: int): std::string

    machine:
        $Judging {
            decide(score: int): std::string {
                if (score >= 60) {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
