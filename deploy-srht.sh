rc-update add docker default
service docker start
cd /home/build/eldemite
mkdir /root/.ssh && cp -r /home/build/.ssh/* /root/.ssh
gem install kamal
export GEM_HOME="$(ruby -e 'puts Gem.user_dir')"
export PATH="$PATH:$GEM_HOME/bin"
set +x
. /home/build/.set_secret_env_vars
set -x
kamal deploy
